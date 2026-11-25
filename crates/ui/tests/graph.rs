use corvo_ui::graph::{GAP, HEIGHT, PAD, SPAN, Shape, Spot, View, WIDTH, ZOOM_MIN, fit, layout};

fn card(children: Vec<Shape>) -> Shape {
    Shape {
        size: (WIDTH, HEIGHT),
        open: !children.is_empty(),
        children,
    }
}

fn leaf() -> Shape {
    card(Vec::new())
}

fn header(spot: &Spot) -> f32 {
    spot.y + HEIGHT / 2.
}

// The middle of the block a card holds.
fn middle(spot: &Spot) -> f32 {
    let (first, last) = (&spot.children[0], spot.children.last().unwrap());
    (first.y + last.y + HEIGHT) / 2.
}

#[test]
fn a_lone_card_sits_at_the_padding() {
    let layout = layout(&leaf());

    assert_eq!((layout.root.x, layout.root.y), (PAD, PAD));
    assert_eq!(layout.width, PAD * 2. + WIDTH);
    assert_eq!(layout.height, PAD * 2. + HEIGHT);
}

#[test]
fn a_card_sits_in_the_middle_of_what_it_holds() {
    let layout = layout(&card(vec![leaf(), leaf(), leaf()]));

    let kids = &layout.root.children;
    assert_eq!(kids[0].x, PAD + WIDTH + SPAN);
    assert_eq!(kids[0].y, PAD);
    assert_eq!(kids[2].y, PAD + 2. * (HEIGHT + GAP));
    assert_eq!(header(&layout.root), middle(&layout.root));
    assert_eq!(layout.height, PAD * 2. + 3. * HEIGHT + 2. * GAP);
}

#[test]
fn a_lone_child_sits_level_with_its_card() {
    let layout = layout(&card(vec![leaf()]));

    assert_eq!(layout.root.children[0].y, layout.root.y);
}

#[test]
fn open_siblings_make_room_for_each_other() {
    // The root holds a folder with three cards, a folder with two, and a file.
    let root = card(vec![
        card(vec![leaf(), leaf(), leaf()]),
        card(vec![leaf(), leaf()]),
        leaf(),
    ]);
    let layout = layout(&root);

    let kids = &layout.root.children;
    let (first, second, third) = (&kids[0], &kids[1], &kids[2]);
    assert_eq!(header(first), middle(first));
    assert_eq!(header(second), middle(second));
    let first_bottom = first.children[2].y + HEIGHT;
    assert_eq!(second.children[0].y, first_bottom + GAP);
    assert_eq!(third.y, second.children[1].y + HEIGHT + GAP);
    assert_eq!(header(&layout.root), middle(&layout.root));
}

#[test]
fn a_dropped_card_widens_its_column_and_pushes_the_cards_under_it() {
    let open = Shape {
        size: (600., HEIGHT + 300.),
        open: true,
        children: vec![leaf()],
    };
    let layout = layout(&card(vec![open, leaf()]));

    let (dropped, under) = (&layout.root.children[0], &layout.root.children[1]);
    assert_eq!(dropped.children[0].y, dropped.y);
    assert_eq!(under.y, dropped.y + HEIGHT + 300. + GAP);
    assert_eq!(dropped.children[0].x, dropped.x + 600. + SPAN);
}

#[test]
fn zooming_keeps_the_point_under_the_mouse() {
    let (before, after) = (View::new(1.), View::new(1.6));
    let mouse = (300., 240.);
    let offset = (-120., -40.);

    let p = before.under(mouse, offset);
    let moved = after.offset(p, mouse);

    let (x, y) = after.at(p);
    assert!((x + moved.0 - mouse.0).abs() < 1e-3);
    assert!((y + moved.1 - mouse.1).abs() < 1e-3);
}

#[test]
fn zoom_scales_graph_space() {
    assert_eq!(View::new(0.5).at((PAD, 2. * PAD)), (PAD / 2., PAD));
}

#[test]
fn a_small_graph_fits_at_zoom_one_in_the_middle() {
    let (zoom, offset) = fit((400., 200.), (1000., 800.));

    assert_eq!(zoom, 1.);
    assert_eq!(offset, (300., 300.));
}

#[test]
fn a_big_graph_zooms_out_to_fit_whole() {
    let (zoom, offset) = fit((2000., 400.), (1000., 800.));

    assert_eq!(zoom, 0.5);
    assert_eq!(offset, (0., 300.));
}

#[test]
fn a_huge_graph_stops_at_the_least_zoom() {
    let (zoom, _) = fit((100_000., 100_000.), (1000., 800.));

    assert_eq!(zoom, ZOOM_MIN);
}
