use std::time::Instant;

use corvo_ui::motion::{Glides, Pan, Sight, TIME, ease};

// A point of the graph, and a card there as it shows: where, and how opaque.
type At = (f32, f32);
type Shown = (At, f32);
// A card to place: its key, where it goes, and the card it comes out of.
type Sent = (&'static str, At, Option<At>);

const HOLDER: At = (24., 100.);
const A: At = (328., 24.);
const B: At = (328., 200.);
const C: At = (632., 200.);

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

// One frame placing the given cards, each with the card it comes out of: where each shows, and
// how opaque, and whether any is still on its way.
fn frame(glides: &mut Glides<&'static str>, cards: &[Sent], now: Instant) -> (Vec<Shown>, bool) {
    let shown = cards
        .iter()
        .map(|(key, to, from)| glides.place(key, *to, *from, now, false))
        .collect();
    (shown, glides.settle(now))
}

#[test]
fn ease_starts_at_nothing_hurries_and_settles_on_one() {
    assert_eq!(ease(0.), 0.);
    assert_eq!(ease(1.), 1.);
    assert!(ease(0.5) > 0.5);
    assert_eq!(ease(-1.), 0.);
    assert_eq!(ease(2.), 1.);
}

#[test]
fn the_first_frame_shows_every_card_where_it_goes() {
    let mut glides = Glides::default();
    let now = Instant::now();

    let (shown, moving) = frame(&mut glides, &[("a", A, Some(HOLDER))], now);

    assert_eq!(shown, vec![(A, 1.)]);
    assert!(!moving);
}

#[test]
fn a_card_sent_elsewhere_glides_there_from_where_it_was() {
    let mut glides = Glides::default();
    let start = Instant::now();
    frame(&mut glides, &[("a", A, None)], start);

    let (shown, moving) = frame(&mut glides, &[("a", B, None)], start);
    assert_eq!(shown, vec![(A, 1.)]);
    assert!(moving);

    let (shown, moving) = frame(&mut glides, &[("a", B, None)], start + TIME / 2);
    let ((x, y), alpha) = shown[0];
    assert_eq!((x, alpha), (A.0, 1.));
    assert!(close(y, A.1 + (B.1 - A.1) * ease(0.5)));
    assert!(moving);

    let (shown, moving) = frame(&mut glides, &[("a", B, None)], start + TIME);
    assert_eq!(shown, vec![(B, 1.)]);
    assert!(!moving);
}

#[test]
fn a_new_card_comes_out_of_the_card_holding_it_fading_in() {
    let mut glides = Glides::default();
    let start = Instant::now();
    frame(&mut glides, &[("dir", HOLDER, None)], start);

    let cards = [("dir", HOLDER, None), ("file", A, Some(HOLDER))];
    let (shown, moving) = frame(&mut glides, &cards, start);
    assert_eq!(shown, vec![(HOLDER, 1.), (HOLDER, 0.)]);
    assert!(moving);

    let (shown, _) = frame(&mut glides, &cards, start + TIME / 2);
    let ((x, _), alpha) = shown[1];
    assert!(x > HOLDER.0 && x < A.0);
    assert!(alpha > 0. && alpha < 1.);

    let (shown, moving) = frame(&mut glides, &cards, start + TIME);
    assert_eq!(shown[1], (A, 1.));
    assert!(!moving);
}

#[test]
fn a_new_card_held_by_none_fades_in_where_it_goes() {
    let mut glides = Glides::default();
    let start = Instant::now();
    frame(&mut glides, &[("a", A, None)], start);

    let (shown, _) = frame(&mut glides, &[("a", A, None), ("b", B, None)], start);

    assert_eq!(shown[1], (B, 0.));
}

#[test]
fn a_card_sent_elsewhere_on_its_way_goes_on_from_where_it_shows() {
    let mut glides = Glides::default();
    let start = Instant::now();
    frame(&mut glides, &[("a", A, None)], start);
    frame(&mut glides, &[("a", B, None)], start);
    let half = start + TIME / 2;
    let (shown, _) = frame(&mut glides, &[("a", B, None)], half);
    let (midway, _) = shown[0];

    let (shown, _) = frame(&mut glides, &[("a", C, None)], half);
    assert_eq!(shown[0], (midway, 1.));

    let (shown, moving) = frame(&mut glides, &[("a", C, None)], half + TIME);
    assert_eq!(shown[0], (C, 1.));
    assert!(!moving);
}

#[test]
fn a_snapped_card_is_just_where_it_goes() {
    let mut glides = Glides::default();
    let now = Instant::now();
    frame(&mut glides, &[("a", A, None)], now);

    assert_eq!(glides.place(&"a", B, None, now, true), (B, 1.));
    assert_eq!(glides.place(&"b", C, Some(B), now, true), (C, 1.));
    assert!(!glides.settle(now));
}

#[test]
fn a_card_gone_and_back_comes_in_again() {
    let mut glides = Glides::default();
    let now = Instant::now();
    frame(
        &mut glides,
        &[("dir", HOLDER, None), ("a", A, Some(HOLDER))],
        now,
    );
    frame(&mut glides, &[("dir", HOLDER, None)], now);

    let (shown, _) = frame(
        &mut glides,
        &[("dir", HOLDER, None), ("a", A, Some(HOLDER))],
        now,
    );

    assert_eq!(shown[1], (HOLDER, 0.));
}

#[test]
fn zooming_with_the_middle_kept_leaves_the_point_there_where_it_is() {
    let start = Instant::now();
    let center = (300., 180.);
    let pan = Pan::new(
        Sight { zoom: 1., center },
        Sight { zoom: 2., center },
        start,
    );

    let mut last = 1.;
    for step in 1..=6 {
        let sight = pan.at(start + TIME * step / 6);
        assert_eq!(sight.center, center);
        assert!(sight.zoom > last || sight.zoom == 2.);
        last = sight.zoom;
    }
    assert_eq!(pan.at(start + TIME), Sight { zoom: 2., center });
    assert!(pan.done(start + TIME));
    assert!(!pan.done(start + TIME / 2));
}

#[test]
fn a_pan_zooms_by_even_ratios_and_slides_the_middle_straight() {
    let start = Instant::now();
    let pan = Pan::new(
        Sight {
            zoom: 0.5,
            center: (0., 0.),
        },
        Sight {
            zoom: 2.,
            center: (100., 50.),
        },
        start,
    );

    let sight = pan.at(start + TIME / 2);
    let t = ease(0.5);

    assert!(close(sight.zoom, 0.5 * 4_f32.powf(t)));
    assert!(close(sight.center.0, 100. * t));
    assert!(close(sight.center.1, 50. * t));
}
