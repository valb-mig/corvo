use std::f32::consts::TAU;
use std::mem;
use std::time::Duration;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, RenderOnce,
    Transformation, Window, div, radians,
};

// A small spinner that turns in steps, a dozen times a second. GPUI's turns smoothly, which asks
// for every frame the display shows, the whole window drawn again each time, for as long as an
// agent works. Every spinner shows the same turn, kept in one entity: a timer moves it on while
// a spinner was drawn since the last step, and the windows that read it draw again then. With
// the desktop's reduced motion on, it stands still.
#[derive(IntoElement, Default)]
pub struct Spinner;

impl Spinner {
    pub fn new() -> Self {
        Self
    }
}

impl RenderOnce for Spinner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let step = Turn::step(cx);
        let angle = TAU * f32::from(step) / f32::from(STEPS);
        div().child(
            Icon::new(Glyph::LoaderCircle)
                .xsmall()
                .transform(Transformation::rotate(radians(angle))),
        )
    }
}

const STEP: Duration = Duration::from_millis(80);
const STEPS: u8 = 12;

#[derive(Default)]
struct Turn {
    step: u8,
    // A spinner was drawn since the last step.
    drawn: bool,
    running: bool,
}

struct Shared(Entity<Turn>);

impl Global for Shared {}

impl Turn {
    // The step every spinner shows now. Reading it ties the window drawing to the next step.
    fn step(cx: &mut App) -> u8 {
        if cx.reduce_motion() {
            return 0;
        }
        let turn = match cx.try_global::<Shared>() {
            Some(shared) => shared.0.clone(),
            None => {
                let turn = cx.new(|_| Turn::default());
                cx.set_global(Shared(turn.clone()));
                turn
            }
        };
        turn.update(cx, |turn, cx| {
            turn.drawn = true;
            if !turn.running {
                turn.running = true;
                cx.spawn(async move |turn, cx| {
                    loop {
                        cx.background_executor().timer(STEP).await;
                        let going = turn.update(cx, Turn::tick).unwrap_or(false);
                        if !going {
                            break;
                        }
                    }
                })
                .detach();
            }
        });
        turn.read(cx).step
    }

    // A step on, when a spinner was drawn since the last; with none, the timer stops until one
    // is drawn again.
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        if !mem::take(&mut self.drawn) {
            self.running = false;
            return false;
        }
        self.step = (self.step + 1) % STEPS;
        cx.notify();
        true
    }
}
