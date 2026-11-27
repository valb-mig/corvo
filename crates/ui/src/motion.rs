use std::collections::HashMap;
use std::hash::Hash;
use std::mem;
use std::time::{Duration, Instant};

use gpui_kit::{Animation, ease_out_quint};

// How the home graph moves instead of jumping: cards glide from where they showed to where the
// layout puts them, new ones coming out of the card that holds them, and the camera glides to
// where the zoom bar sends it. The glides know nothing about the window: the time comes in, so
// they can be driven by hand.

// How long a glide takes.
pub const TIME: Duration = Duration::from_millis(180);

// How what opens over the window comes in (a window over the home, its palette, a drawer):
// quickly, settling softly. GPUI runs it, and with the desktop's reduced motion on, shows it
// already in place.
pub fn opening() -> Animation {
    Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint())
}

// Fast at first, settling softly: an ease-out cubic over 0..=1.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t).powi(3)
}

// How far along a glide started at `start` is at `now`, eased.
fn along(start: Instant, now: Instant) -> f32 {
    ease(now.saturating_duration_since(start).as_secs_f32() / TIME.as_secs_f32())
}

fn done(start: Instant, now: Instant) -> bool {
    now.saturating_duration_since(start) >= TIME
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t))
}

// A card on its way: from where it showed, and how opaque, to where it goes, fully shown.
#[derive(Clone, Copy, Debug)]
struct Glide {
    from: (f32, f32),
    alpha: f32,
    to: (f32, f32),
    start: Instant,
}

impl Glide {
    fn at(&self, now: Instant) -> ((f32, f32), f32) {
        let t = along(self.start, now);
        (mix(self.from, self.to, t), lerp(self.alpha, 1., t))
    }
}

// Where each card in sight was last sent, and the cards still on their way, by key. Each frame
// places every card in sight, those that hold others before what they hold, then settles.
pub struct Glides<K> {
    aims: HashMap<K, (f32, f32)>,
    next: HashMap<K, (f32, f32)>,
    glides: HashMap<K, Glide>,
    // A frame was placed before: the first one shows every card where it goes.
    seen: bool,
}

impl<K> Default for Glides<K> {
    fn default() -> Self {
        Self {
            aims: HashMap::new(),
            next: HashMap::new(),
            glides: HashMap::new(),
            seen: false,
        }
    }
}

impl<K: Eq + Hash + Clone> Glides<K> {
    // Where the card sent to `to` shows at `now`, and how opaque it is. One sent elsewhere than
    // the last frame glides there from where it shows; a new one comes out of `from`, the card
    // holding it, fading in, or fades in where it goes. With `snap`, a card sent elsewhere or new
    // is just there.
    pub fn place(
        &mut self,
        key: &K,
        to: (f32, f32),
        from: Option<(f32, f32)>,
        now: Instant,
        snap: bool,
    ) -> ((f32, f32), f32) {
        let shows = match self.glides.get(key) {
            Some(glide) => Some(glide.at(now)),
            None => self.aims.get(key).map(|aim| (*aim, 1.)),
        };
        let moved = self.aims.get(key) != Some(&to);
        self.next.insert(key.clone(), to);
        let glide = match shows {
            Some(shows) if !moved => return shows,
            _ if snap || !self.seen => {
                self.glides.remove(key);
                return (to, 1.);
            }
            Some((at, alpha)) => Glide {
                from: at,
                alpha,
                to,
                start: now,
            },
            None => Glide {
                from: from.unwrap_or(to),
                alpha: 0.,
                to,
                start: now,
            },
        };
        self.glides.insert(key.clone(), glide);
        (glide.from, glide.alpha)
    }

    // Ends the frame: a card not placed in it is gone. True while a card is still on its way.
    pub fn settle(&mut self, now: Instant) -> bool {
        self.aims = mem::take(&mut self.next);
        let aims = &self.aims;
        self.glides
            .retain(|key, glide| aims.contains_key(key) && !done(glide.start, now));
        self.seen = true;
        !self.glides.is_empty()
    }
}

// Where a view looks from: how close, and the graph point in the middle of the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sight {
    pub zoom: f32,
    pub center: (f32, f32),
}

// The camera on its way from one sight to another. The zoom changes by the same ratio at each
// moment, so it reads evenly however far it goes, and the middle slides straight: zooming with
// the middle kept, the point there stays put.
#[derive(Clone, Copy, Debug)]
pub struct Pan {
    from: Sight,
    to: Sight,
    start: Instant,
}

impl Pan {
    pub fn new(from: Sight, to: Sight, start: Instant) -> Self {
        Self { from, to, start }
    }

    pub fn to(&self) -> Sight {
        self.to
    }

    pub fn at(&self, now: Instant) -> Sight {
        if self.done(now) {
            return self.to;
        }
        let t = along(self.start, now);
        Sight {
            zoom: self.from.zoom * (self.to.zoom / self.from.zoom).powf(t),
            center: mix(self.from.center, self.to.center, t),
        }
    }

    pub fn done(&self, now: Instant) -> bool {
        done(self.start, now)
    }
}
