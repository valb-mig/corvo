use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::panic;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, CursorStyle, Div, Element, ElementId, FocusHandle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, InteractiveElement as _, IntoElement, LayoutId,
    MouseButton, ParentElement as _, PathBuilder, Pixels, Point, RenderOnce, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point,
    prelude::FluentBuilder as _, px,
};

use crate::motion::{Glides, Pan, Sight};

// Cards as a tree read left to right: an open card holds its cards in the next column, with a
// curve to each, and any number of cards can be open at once. A column can come in blocks, a
// little apart, and end in a chip; a card can stand for a pile, with sheets under it, the wheel
// over it turning through what it holds. A card can drop a panel down from
// under its header; its column widens to hold it. Cards can be picked, with Ctrl or Shift on a
// click or a box drawn on the background, to group or hide them together; a group is a frame
// around its cards, or one card in their place when collapsed. The wheel moves the graph, and so
// does a drag with Space held, anywhere on it. Nothing jumps: cards glide to where the
// layout puts them, new ones coming out of the card holding them, and the zoom bar glides the
// camera; the wheel and the mouse move it at once.

pub const WIDTH: f32 = 232.;
pub const HEIGHT: f32 = 52.;
pub const GAP: f32 = 10.;
pub const SPAN: f32 = 72.;
pub const PAD: f32 = 24.;
pub const ZOOM_MIN: f32 = 0.3;
pub const ZOOM_MAX: f32 = 2.;
// The room a group's frame leaves around its cards, and the height of its title.
pub const RIM: f32 = 12.;
pub const TITLE: f32 = 28.;
// The room between blocks of a column, how far the sheets of a pile show under it, the height
// of a chip, and the room between the sheets of a pile and its chip.
pub const BLOCK: f32 = 22.;
pub const SHEETS: f32 = 10.;
pub const CHIP: f32 = 28.;
pub const UNDER: f32 = 6.;

const ICON: f32 = 16.;
const STEP: f32 = 1.2;
// How far past the viewport cards are still drawn, in pixels, so those scrolling in are there.
pub const AROUND: f32 = 240.;
// How far the wheel goes to turn a pile by one: a notch of a mouse wheel.
const TURN: f32 = 48.;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type Turn = Rc<dyn Fn(i32, &mut Window, &mut App)>;
type Draw = Box<dyn Fn(Pixels) -> AnyElement>;
type Keys = Rc<dyn Fn(Vec<SharedString>, &mut Window, &mut App)>;
type Moves = Rc<dyn Fn(Vec<(SharedString, (f32, f32))>, &mut Window, &mut App)>;

// A box in graph space: left, top, width and height.
pub type Rect = (f32, f32, f32, f32);

pub struct Card {
    id: SharedString,
    icon: Option<Draw>,
    title: SharedString,
    meta: Option<SharedString>,
    badge: Option<SharedString>,
    muted: bool,
    // A color of its own for the title, and a light fill of it behind the card.
    tint: Option<(Hsla, bool)>,
    selectable: bool,
    on_click: Option<Handler>,
    on_turn: Option<Turn>,
    actions: Vec<Action>,
    drop: Option<Drop>,
    form: Form,
    // The chip under the sheets of a pile.
    under: Option<Under>,
}

// How a card is drawn: plain, as the top of a pile with its sheets and a chip under it, or as a
// chip at the end of its column, with no curve to it and its holder not sitting level with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Form {
    #[default]
    Plain,
    Pile,
    Chip,
}

struct Under {
    label: SharedString,
    run: Handler,
}

// An entry of the menu a right click on the card opens.
struct Action {
    label: SharedString,
    glyph: Glyph,
    run: Handler,
}

struct Drop {
    element: AnyElement,
    width: f32,
    height: f32,
}

impl Card {
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            title: title.into(),
            meta: None,
            badge: None,
            muted: false,
            tint: None,
            selectable: true,
            on_click: None,
            on_turn: None,
            actions: Vec::new(),
            drop: None,
            form: Form::Plain,
            under: None,
        }
    }

    pub fn id(&self) -> &SharedString {
        &self.id
    }

    // The icon, drawn at the size the zoom asks for.
    pub fn icon<E: IntoElement>(mut self, draw: impl Fn(Pixels) -> E + 'static) -> Self {
        self.icon = Some(Box::new(move |size| draw(size).into_any_element()));
        self
    }

    pub fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    pub fn tint(mut self, color: Hsla, fill: bool) -> Self {
        self.tint = Some((color, fill));
        self
    }

    // Whether it can be picked to act on with others; a note can't.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        glyph: Glyph,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.actions.push(Action {
            label: label.into(),
            glyph,
            run: Rc::new(run),
        });
        self
    }

    // A panel under the header, of the given size at zoom 1. Its sizes inside go in rems, so
    // they follow the zoom.
    pub fn dropdown(mut self, element: impl IntoElement, width: f32, height: f32) -> Self {
        self.drop = Some(Drop {
            element: element.into_any_element(),
            width,
            height,
        });
        self
    }

    // Drawn as the top of a pile, and under its sheets a chip doing what its label says, in
    // sight whatever the pile is turned to. Neither a pile nor a chip can be picked.
    pub fn pile(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.form = Form::Pile;
        self.under = Some(Under {
            label: label.into(),
            run: Rc::new(run),
        });
        self.selectable = false;
        self
    }

    // What the wheel over the card does, by how many it turns: down is forward. With Ctrl the
    // wheel still zooms, and sideways it still moves the graph.
    pub fn on_turn(mut self, f: impl Fn(i32, &mut Window, &mut App) + 'static) -> Self {
        self.on_turn = Some(Rc::new(f));
        self
    }

    // Drawn as a chip at the end of its column: its icon and title, a click doing what it says.
    pub fn chip(mut self) -> Self {
        self.form = Form::Chip;
        self.selectable = false;
        self
    }

    fn size(&self) -> (f32, f32) {
        match (&self.drop, self.form) {
            (Some(drop), _) => (drop.width.max(WIDTH), HEIGHT + drop.height),
            (None, Form::Pile) => (WIDTH, HEIGHT + SHEETS + UNDER + CHIP),
            (None, Form::Chip) => (WIDTH, CHIP),
            (None, Form::Plain) => (WIDTH, HEIGHT),
        }
    }
}

// Cards put together under a title, each known by its id: a frame around those in sight and all
// they hold, or one card in their place when collapsed. Dragging the title, or that card, moves
// them all.
pub struct Group {
    id: SharedString,
    title: SharedString,
    members: Vec<SharedString>,
    collapsed: bool,
    editor: Option<AnyElement>,
    on_click: Option<Handler>,
    actions: Vec<Action>,
}

impl Group {
    pub fn new(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        members: impl IntoIterator<Item = SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            members: members.into_iter().collect(),
            collapsed: false,
            editor: None,
            on_click: None,
            actions: Vec::new(),
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    // Shown in place of the title, to type a new one in.
    pub fn editor(mut self, editor: impl IntoElement) -> Self {
        self.editor = Some(editor.into_any_element());
        self
    }

    // What a click on the card of the collapsed group does.
    pub fn on_click(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        glyph: Glyph,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.actions.push(Action {
            label: label.into(),
            glyph,
            run: Rc::new(run),
        });
        self
    }

    // The card that stands for the group while it is collapsed.
    fn stand_in(self, count: usize) -> Card {
        let meta = match count {
            1 => "1 card".to_owned(),
            n => format!("{n} cards"),
        };
        Card {
            id: self.id,
            icon: Some(Box::new(|size| {
                Icon::new(Glyph::Group).size(size).into_any_element()
            })),
            title: self.title,
            meta: Some(meta.into()),
            badge: None,
            muted: false,
            tint: None,
            selectable: false,
            on_click: self.on_click,
            on_turn: None,
            actions: self.actions,
            drop: None,
            form: Form::Plain,
            under: None,
        }
    }
}

// A card and, when it is open, the cards it holds.
pub struct Branch {
    card: Card,
    open: bool,
    apart: bool,
    children: Vec<Branch>,
}

impl Branch {
    pub fn new(card: Card) -> Self {
        Self {
            card,
            open: false,
            apart: false,
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    // Starts a block of its column, a little apart from the cards above it.
    pub fn apart(mut self, apart: bool) -> Self {
        self.apart = apart;
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Branch>) -> Self {
        self.children.extend(children);
        self
    }
}

// A branch as the size of its card, whether it is open, and what it holds; whether it starts a
// block of its column, and whether it trails the column, as a chip does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub size: (f32, f32),
    pub open: bool,
    pub apart: bool,
    pub trails: bool,
    pub children: Vec<Shape>,
}

// Where a card goes, and the cards it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    pub children: Vec<Spot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub roots: Vec<Spot>,
    pub width: f32,
    pub height: f32,
}

// Every depth is a column as wide as its widest card. A card sits halfway between the first and
// the last card it holds, past any that trail, and each branch takes the height of all it holds,
// so the cards around it make room and nothing overlaps however much is open. A block starts
// further down than a card would. The roots stack down the first column the same way.
pub fn layout(roots: &[Shape]) -> Layout {
    let mut widths = Vec::new();
    for root in roots {
        measure(root, 0, &mut widths);
    }
    let mut lefts = Vec::with_capacity(widths.len());
    let mut x = PAD;
    for width in &widths {
        lefts.push(x);
        x += width + SPAN;
    }
    let mut top = PAD;
    let mut spots = Vec::with_capacity(roots.len());
    for root in roots {
        let (spot, bottom) = place(root, 0, top, &lefts);
        spots.push(spot);
        top = bottom + GAP;
    }
    Layout {
        roots: spots,
        width: (x - SPAN).max(PAD) + PAD,
        height: (top - GAP).max(PAD) + PAD,
    }
}

fn measure(shape: &Shape, depth: usize, widths: &mut Vec<f32>) {
    if widths.len() <= depth {
        widths.push(WIDTH);
    }
    widths[depth] = widths[depth].max(shape.size.0);
    for child in &shape.children {
        measure(child, depth + 1, widths);
    }
}

fn place(shape: &Shape, depth: usize, top: f32, lefts: &[f32]) -> (Spot, f32) {
    let x = lefts[depth];
    if shape.children.is_empty() {
        return (
            Spot {
                x,
                y: top,
                children: Vec::new(),
            },
            top + shape.size.1,
        );
    }
    // What it holds stacks down from `top`, each branch past the one before.
    let mut low = top;
    let mut children = Vec::with_capacity(shape.children.len());
    for (ix, child) in shape.children.iter().enumerate() {
        if ix > 0 && child.apart {
            low += BLOCK - GAP;
        }
        let (spot, bottom) = place(child, depth + 1, low, lefts);
        children.push(spot);
        low = bottom + GAP;
    }
    // The card sits halfway between the first card it holds and the last that doesn't trail.
    let mut level = children
        .iter()
        .zip(&shape.children)
        .filter(|(_, child)| !child.trails)
        .map(|(spot, _)| spot.y);
    let first = level.next().unwrap_or(children[0].y);
    let last = level.next_back().unwrap_or(first);
    let y = (first + last) / 2.;
    let bottom = (y + shape.size.1).max(low - GAP);
    (Spot { x, y, children }, bottom)
}

// Where graph space lands in the scrolled content: scaled by the zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub zoom: f32,
}

impl View {
    pub fn new(zoom: f32) -> Self {
        Self { zoom }
    }

    pub fn at(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (x * self.zoom, y * self.zoom)
    }

    // The graph point under a point of the viewport, scrolled by `offset`.
    pub fn under(&self, (x, y): (f32, f32), offset: (f32, f32)) -> (f32, f32) {
        ((x - offset.0) / self.zoom, (y - offset.1) / self.zoom)
    }

    // The offset that puts the graph point `p` under the viewport point `at`.
    pub fn offset(&self, p: (f32, f32), at: (f32, f32)) -> (f32, f32) {
        let (x, y) = self.at(p);
        (at.0 - x, at.1 - y)
    }
}

// Where a graph is looked at from, free of any edge: how far it is moved and how close, and the
// cards picked. It lives with the graph's owner between renders.
#[derive(Clone, Default)]
pub struct Camera {
    lens: Rc<RefCell<Lens>>,
}

struct Lens {
    zoom: f32,
    offset: (f32, f32),
    // The viewport, as last painted.
    bounds: Bounds<Pixels>,
    drag: Option<Drag>,
    // The press went to a drag or a pick, so the click it ends in is not a click.
    spent: bool,
    // The cards picked to act on together.
    picked: Vec<SharedString>,
    // As last laid out: how far each card was moved by hand, the box of each card that can be
    // picked, and the card that holds each.
    nudges: HashMap<SharedString, (f32, f32)>,
    boxes: Vec<(SharedString, Rect)>,
    holders: HashMap<SharedString, SharedString>,
    // The size of the graph at zoom 1, as last laid out.
    extent: (f32, f32),
    // Fit the graph to the viewport on the next paint, once its size is known.
    fit: bool,
    // The card the wheel is turning, and how far it went short of a turn.
    spin: Option<(SharedString, f32)>,
    // The cards on their way to where the layout puts them, and the camera on its way somewhere.
    glides: Glides<SharedString>,
    pan: Option<Pan>,
    // Motion is turned down: nothing glides.
    still: bool,
    // Space is held: a drag anywhere moves the view.
    hand: bool,
}

#[derive(Clone)]
enum Drag {
    // Where the mouse was pressed, and the offset then.
    View(Point<Pixels>, (f32, f32)),
    // Where the mouse was pressed, the cards moved together with how far each was moved before,
    // and how far they went since, in graph space.
    Cards(Point<Pixels>, Vec<(SharedString, (f32, f32))>, (f32, f32)),
    // A box picking every card it touches, from where it was pressed to the mouse, in graph space.
    Pick((f32, f32), (f32, f32)),
}

impl Default for Lens {
    fn default() -> Self {
        Self {
            zoom: 1.,
            offset: (0., 0.),
            bounds: Bounds::default(),
            drag: None,
            spent: false,
            picked: Vec::new(),
            nudges: HashMap::new(),
            boxes: Vec::new(),
            holders: HashMap::new(),
            extent: (0., 0.),
            fit: false,
            spin: None,
            glides: Glides::default(),
            pan: None,
            still: false,
            hand: false,
        }
    }
}

impl Lens {
    fn middle(&self) -> (f32, f32) {
        let size = self.bounds.size;
        (f32::from(size.width) / 2., f32::from(size.height) / 2.)
    }

    fn sight(&self) -> Sight {
        Sight {
            zoom: self.zoom,
            center: View::new(self.zoom).under(self.middle(), self.offset),
        }
    }

    // Looks from the given sight at once.
    fn look(&mut self, sight: Sight) {
        self.zoom = sight.zoom;
        self.offset = View::new(sight.zoom).offset(sight.center, self.middle());
    }

    // Heads for the given sight, gliding from where it looks now unless motion is turned down.
    fn head(&mut self, to: Sight) {
        if self.still {
            self.pan = None;
            self.look(to);
            return;
        }
        self.pan = Some(Pan::new(self.sight(), to, Instant::now()));
    }

    // Where it is heading, or where it looks from when going nowhere.
    fn aim(&self) -> Sight {
        self.pan.map_or_else(|| self.sight(), |pan| pan.to())
    }
}

impl Camera {
    pub fn zoom(&self) -> f32 {
        self.lens.borrow().zoom
    }

    pub fn offset(&self) -> (f32, f32) {
        self.lens.borrow().offset
    }

    // The wheel and the mouse move the camera at once, stopping any glide.
    fn move_by(&self, (dx, dy): (f32, f32)) {
        let mut lens = self.lens.borrow_mut();
        lens.pan = None;
        lens.offset = (lens.offset.0 + dx, lens.offset.1 + dy);
    }

    // Zooms at once keeping the graph point under the viewport point `at` where it is.
    fn zoom_to(&self, zoom: f32, at: (f32, f32)) {
        let mut lens = self.lens.borrow_mut();
        lens.pan = None;
        let p = View::new(lens.zoom).under(at, lens.offset);
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        lens.zoom = zoom;
        lens.offset = View::new(zoom).offset(p, at);
    }

    // Glides closer or further by `by`, the middle of the viewport kept; asked again on the way,
    // it goes on from where it was heading.
    fn glide_zoom(&self, by: f32) {
        let mut lens = self.lens.borrow_mut();
        let aim = lens.aim();
        let zoom = (aim.zoom * by).clamp(ZOOM_MIN, ZOOM_MAX);
        lens.head(Sight { zoom, ..aim });
    }

    // Glides back to where it started: zoom 1, nothing moved.
    fn reset(&self) {
        let mut lens = self.lens.borrow_mut();
        let center = lens.middle();
        lens.head(Sight { zoom: 1., center });
    }

    // Moves the camera along its glide; true while it is on its way.
    fn step(&self, now: Instant, still: bool) -> bool {
        let mut lens = self.lens.borrow_mut();
        lens.still = still;
        let Some(pan) = lens.pan else {
            return false;
        };
        if still || pan.done(now) {
            lens.look(pan.to());
            lens.pan = None;
            return false;
        }
        lens.look(pan.at(now));
        true
    }

    // Where a card in sight shows now on its way to `to`, and how opaque; see `Glides::place`.
    fn place(
        &self,
        key: &SharedString,
        to: (f32, f32),
        from: Option<(f32, f32)>,
        now: Instant,
        snap: bool,
    ) -> ((f32, f32), f32) {
        let mut lens = self.lens.borrow_mut();
        let snap = snap || lens.still;
        lens.glides.place(key, to, from, now, snap)
    }

    // Every card in sight was placed: true while one is still on its way.
    fn settle(&self, now: Instant) -> bool {
        self.lens.borrow_mut().glides.settle(now)
    }

    // A window point in the viewport.
    fn local(&self, at: Point<Pixels>) -> (f32, f32) {
        let at = at - self.lens.borrow().bounds.origin;
        (f32::from(at.x), f32::from(at.y))
    }

    // The graph point under a window point.
    fn under(&self, at: Point<Pixels>) -> (f32, f32) {
        let local = self.local(at);
        View::new(self.zoom()).under(local, self.offset())
    }

    fn spent(&self) -> bool {
        self.lens.borrow().spent
    }

    fn hand(&self) -> bool {
        self.lens.borrow().hand
    }

    // Space went down or up; true when that changes what a drag does.
    fn hold(&self, hand: bool) -> bool {
        let mut lens = self.lens.borrow_mut();
        let changed = lens.hand != hand;
        lens.hand = hand;
        changed
    }

    // The wheel went `by` over a card: how many turns that makes with what it went before over
    // the same card, down being forward.
    fn turns(&self, id: &SharedString, by: f32) -> i32 {
        let mut lens = self.lens.borrow_mut();
        let went = match &lens.spin {
            Some((card, went)) if card == id => *went,
            _ => 0.,
        } - by;
        let turns = (went / TURN).trunc();
        lens.spin = Some((id.clone(), went - turns * TURN));
        turns as i32
    }

    fn drag(&self) -> Option<Drag> {
        self.lens.borrow().drag.clone()
    }

    fn is_picked(&self, id: &SharedString) -> bool {
        self.lens.borrow().picked.contains(id)
    }

    // The cards picked, when the given one is among two or more of them.
    fn picked_with(&self, id: &SharedString) -> Option<Vec<SharedString>> {
        let lens = self.lens.borrow();
        (lens.picked.len() > 1 && lens.picked.contains(id)).then(|| lens.picked.clone())
    }

    pub fn unpick(&self) {
        self.lens.borrow_mut().picked.clear();
    }

    // A press on a card's header. With `toggle` it picks the card, or lets it go; otherwise the
    // card goes along when the mouse moves, with the others picked when it is one of them.
    fn grab(&self, id: &SharedString, at: Point<Pixels>, toggle: bool) {
        let mut lens = self.lens.borrow_mut();
        lens.pan = None;
        lens.spent = false;
        if toggle {
            match lens.picked.iter().position(|picked| picked == id) {
                Some(ix) => {
                    lens.picked.remove(ix);
                }
                None => lens.picked.push(id.clone()),
            }
            lens.spent = true;
            lens.drag = None;
            return;
        }
        if !lens.picked.contains(id) {
            lens.picked.clear();
        }
        let keys = if lens.picked.is_empty() {
            vec![id.clone()]
        } else {
            heads(&lens.picked, &lens.holders)
        };
        let from = keys
            .into_iter()
            .map(|key| {
                let by = lens.nudges.get(&key).copied().unwrap_or_default();
                (key, by)
            })
            .collect();
        lens.drag = Some(Drag::Cards(at, from, (0., 0.)));
    }

    // A press that takes the given cards along when the mouse moves: a group's.
    fn carry(&self, keys: &[SharedString], at: Point<Pixels>) {
        let mut lens = self.lens.borrow_mut();
        let from = keys
            .iter()
            .map(|key| {
                (
                    key.clone(),
                    lens.nudges.get(key).copied().unwrap_or_default(),
                )
            })
            .collect();
        lens.pan = None;
        lens.spent = false;
        lens.drag = Some(Drag::Cards(at, from, (0., 0.)));
    }

    // A press on the background draws a box to pick cards with, or with Space held, a press
    // anywhere moves the view.
    fn press(&self, at: Point<Pixels>, pick: bool) {
        let p = self.under(at);
        let mut lens = self.lens.borrow_mut();
        lens.pan = None;
        lens.spent = false;
        lens.drag = Some(if pick {
            lens.picked.clear();
            Drag::Pick(p, p)
        } else {
            Drag::View(at, lens.offset)
        });
    }

    // Follows the held mouse with what was pressed; false when nothing moves.
    fn follow(&self, at: Point<Pixels>) -> bool {
        let p = self.under(at);
        let mut lens = self.lens.borrow_mut();
        let zoom = lens.zoom;
        let Some(drag) = lens.drag.clone() else {
            return false;
        };
        match drag {
            Drag::View(start, from) => {
                let (dx, dy) = apart(start, at);
                lens.spent |= dx.abs() + dy.abs() > 4.;
                lens.offset = (from.0 + dx, from.1 + dy);
            }
            Drag::Cards(start, from, _) => {
                let (dx, dy) = apart(start, at);
                lens.spent |= dx.abs() + dy.abs() > 4.;
                lens.drag = Some(Drag::Cards(start, from, (dx / zoom, dy / zoom)));
            }
            Drag::Pick(start, _) => {
                let area = span(start, p);
                lens.picked = lens
                    .boxes
                    .iter()
                    .filter(|(_, rect)| touches(area, *rect))
                    .map(|(id, _)| id.clone())
                    .collect();
                lens.spent = true;
                lens.drag = Some(Drag::Pick(start, p));
            }
        }
        true
    }

    // Ends the press: a click on the background lets the picked cards go, and cards dragged give
    // where each was moved to.
    fn release(&self) -> Option<Vec<(SharedString, (f32, f32))>> {
        let mut lens = self.lens.borrow_mut();
        match lens.drag.take()? {
            Drag::View(..) => {
                if !lens.spent {
                    lens.picked.clear();
                }
                None
            }
            Drag::Pick(..) => None,
            Drag::Cards(_, from, by) => lens.spent.then(|| {
                from.into_iter()
                    .map(|(key, at)| (key, (at.0 + by.0, at.1 + by.1)))
                    .collect()
            }),
        }
    }

    // Keeps what the last layout knows, for the presses to come.
    fn remember(
        &self,
        nudges: HashMap<SharedString, (f32, f32)>,
        boxes: Vec<(SharedString, Rect)>,
        holders: HashMap<SharedString, SharedString>,
        extent: (f32, f32),
    ) {
        let mut lens = self.lens.borrow_mut();
        // A picked card no longer in sight is let go, unless a box is still picking.
        if !matches!(lens.drag, Some(Drag::Pick(..))) {
            lens.picked
                .retain(|picked| boxes.iter().any(|(id, _)| id == picked));
        }
        lens.nudges = nudges;
        lens.boxes = boxes;
        lens.holders = holders;
        lens.extent = extent;
    }

    // The whole graph centred in view on the next paint.
    pub fn center(&self) {
        self.lens.borrow_mut().fit = true;
    }

    // Records the viewport, gliding to fit the graph in it when asked: whether the camera moved,
    // and whether the viewport changed size, so other cards are in sight.
    fn frame(&self, bounds: Bounds<Pixels>) -> (bool, bool) {
        let mut lens = self.lens.borrow_mut();
        let resized = lens.bounds.size != bounds.size;
        lens.bounds = bounds;
        let viewport = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if !lens.fit || viewport.0 <= 0. || viewport.1 <= 0. {
            return (false, resized);
        }
        let (zoom, offset) = fit(lens.extent, viewport);
        let center = View::new(zoom).under(lens.middle(), offset);
        lens.head(Sight { zoom, center });
        lens.fit = false;
        (true, resized)
    }

    // The part of the graph in sight, in graph space, with room around it for what scrolls or
    // glides in next; none before the viewport is known, when everything is in sight.
    fn in_sight(&self) -> Option<Rect> {
        let lens = self.lens.borrow();
        let size = lens.bounds.size;
        let viewport = (f32::from(size.width), f32::from(size.height));
        sight(View::new(lens.zoom), lens.offset, viewport)
    }
}

// The part of the graph a viewport of the given size shows, scrolled by `offset`, in graph space
// and with AROUND pixels of room on every side; none for an empty viewport.
pub fn sight(view: View, offset: (f32, f32), (width, height): (f32, f32)) -> Option<Rect> {
    if width <= 0. || height <= 0. {
        return None;
    }
    let (left, top) = view.under((-AROUND, -AROUND), offset);
    let (right, bottom) = view.under((width + AROUND, height + AROUND), offset);
    Some((left, top, right - left, bottom - top))
}

fn apart(start: Point<Pixels>, at: Point<Pixels>) -> (f32, f32) {
    let delta = at - start;
    (f32::from(delta.x), f32::from(delta.y))
}

// The box around the given boxes; none when there are none.
pub fn hull(boxes: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    let mut boxes = boxes.into_iter();
    let (x, y, w, h) = boxes.next()?;
    let (mut left, mut top, mut right, mut bottom) = (x, y, x + w, y + h);
    for (x, y, w, h) in boxes {
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + w);
        bottom = bottom.max(y + h);
    }
    Some((left, top, right - left, bottom - top))
}

// The box between two corners, in any order.
pub fn span(a: (f32, f32), b: (f32, f32)) -> Rect {
    (
        a.0.min(b.0),
        a.1.min(b.1),
        (a.0 - b.0).abs(),
        (a.1 - b.1).abs(),
    )
}

pub fn touches(a: Rect, b: Rect) -> bool {
    a.0 <= b.0 + b.2 && b.0 <= a.0 + a.2 && a.1 <= b.1 + b.3 && b.1 <= a.1 + a.3
}

// The frame of a group around the box of its cards: room on every side, and its title on top.
pub fn frame((x, y, w, h): Rect) -> Rect {
    (x - RIM, y - RIM - TITLE, w + 2. * RIM, h + 2. * RIM + TITLE)
}

// Those of the keys that no other of them holds, however far up: moving them moves the rest
// along, so nothing moves twice.
pub fn heads(
    keys: &[SharedString],
    holders: &HashMap<SharedString, SharedString>,
) -> Vec<SharedString> {
    let all: HashSet<&SharedString> = keys.iter().collect();
    keys.iter()
        .filter(|key| {
            let mut up = holders.get(*key);
            while let Some(holder) = up {
                if all.contains(holder) {
                    return false;
                }
                up = holders.get(holder);
            }
            true
        })
        .cloned()
        .collect()
}

// The zoom and offset that show a graph of the given size whole and centred in the viewport,
// never closer than zoom 1.
pub fn fit(extent: (f32, f32), viewport: (f32, f32)) -> (f32, (f32, f32)) {
    let (w, h) = (extent.0.max(1.), extent.1.max(1.));
    let zoom = (viewport.0 / w)
        .min(viewport.1 / h)
        .min(1.)
        .clamp(ZOOM_MIN, ZOOM_MAX);
    let offset = ((viewport.0 - w * zoom) / 2., (viewport.1 - h * zoom) / 2.);
    (zoom, offset)
}

// Lays a child out and paints it with the rem scaled, so whatever it sizes in rems follows.
struct Scaled {
    scale: f32,
    child: AnyElement,
}

impl IntoElement for Scaled {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Scaled {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let rem = window.rem_size() * self.scale;
        let id = window.with_rem_size(Some(rem), |window| self.child.request_layout(window, cx));
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem = window.rem_size() * self.scale;
        window.with_rem_size(Some(rem), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem = window.rem_size() * self.scale;
        window.with_rem_size(Some(rem), |window| self.child.paint(window, cx));
    }
}

// Painted in this order, each over the ones before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Stroke {
    Tree,
    Lit,
}

struct Edge {
    from: (f32, f32),
    to: (f32, f32),
    stroke: Stroke,
    // Fading in with the card it goes to.
    alpha: f32,
}

// The two points a curve from `from` to `to` bends toward, leaving its card and coming into the
// next.
fn bends(from: (f32, f32), to: (f32, f32)) -> ((f32, f32), (f32, f32)) {
    let bend = ((to.0 - from.0) / 2.).max(SPAN / 2.);
    ((from.0 + bend, from.1), (to.0 - bend, to.1))
}

// The box a curve from `from` to `to` stays in: that of its ends and the points it bends toward,
// which go past the ends when the cards are close.
pub fn reach(from: (f32, f32), to: (f32, f32)) -> Rect {
    let (a, b) = bends(from, to);
    let corners = [from, to, a, b].map(|(x, y)| (x, y, 0., 0.));
    hull(corners).unwrap_or_default()
}

#[derive(IntoElement)]
pub struct Graph {
    id: ElementId,
    camera: Camera,
    focus: Option<FocusHandle>,
    roots: Vec<Branch>,
    nudges: HashMap<SharedString, (f32, f32)>,
    groups: Vec<Group>,
    on_move: Option<Moves>,
    on_group: Option<Keys>,
    on_hide: Option<Keys>,
    on_reorganize: Option<Handler>,
}

impl Graph {
    pub fn new(
        id: impl Into<ElementId>,
        camera: &Camera,
        roots: impl IntoIterator<Item = Branch>,
    ) -> Self {
        Self {
            id: id.into(),
            camera: camera.clone(),
            focus: None,
            roots: roots.into_iter().collect(),
            nudges: HashMap::new(),
            groups: Vec::new(),
            on_move: None,
            on_group: None,
            on_hide: None,
            on_reorganize: None,
        }
    }

    // Where the keys go while the graph is in front, so Space held turns a drag into moving the
    // view. A press on the graph takes them, unless something inside it has them.
    pub fn focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    // How far each card was moved by hand, by its id; what it holds moves with it.
    pub fn nudges(mut self, nudges: HashMap<SharedString, (f32, f32)>) -> Self {
        self.nudges = nudges;
        self
    }

    pub fn groups(mut self, groups: impl IntoIterator<Item = Group>) -> Self {
        self.groups.extend(groups);
        self
    }

    // Cards were dragged: where each of those moved is now moved to.
    pub fn on_move(
        mut self,
        f: impl Fn(Vec<(SharedString, (f32, f32))>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }

    // Asked from the menu of picked cards: put them in a group.
    pub fn on_group(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_group = Some(Rc::new(f));
        self
    }

    // Asked from the menu of picked cards: hide them all.
    pub fn on_hide(
        mut self,
        f: impl Fn(Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_hide = Some(Rc::new(f));
        self
    }

    // Asked from the zoom bar, once cards were moved: put them back where the layout says.
    pub fn on_reorganize(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_reorganize = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Graph {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let now = Instant::now();
        let camera = self.camera;
        let panning = camera.step(now, cx.reduce_motion());
        // Space let go while the keys were elsewhere never comes up here.
        let keyed = self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window));
        if !keyed || !window.is_window_active() {
            camera.hold(false);
        }
        let theme = cx.theme();
        let shapes: Vec<Shape> = self.roots.iter().map(shape).collect();
        let layout = layout(&shapes);
        let view = View::new(camera.zoom());
        let zoom = view.zoom;
        let (line, lit) = (theme.muted_foreground.opacity(0.45), theme.blue);
        let drag = camera.drag();

        // Where each card goes: the layout, moved by hand and by the drag going on.
        let mut nudges = self.nudges.clone();
        if let Some(Drag::Cards(_, from, by)) = &drag {
            for (key, at) in from {
                nudges.insert(key.clone(), (at.0 + by.0, at.1 + by.1));
            }
        }
        let mut placed = Vec::new();
        let mut placing = Placing {
            nudges: &nudges,
            placed: &mut placed,
        };
        for ((root, shape), spot) in self.roots.into_iter().zip(&shapes).zip(&layout.roots) {
            placing.lay(root, shape, spot, (0., 0.), None);
        }

        // The collapsed group each card is out of sight in: that of a card holding it, or its own.
        let folds: HashMap<&SharedString, usize> = self
            .groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.collapsed)
            .flat_map(|(gx, group)| group.members.iter().map(move |id| (id, gx)))
            .collect();
        let mut tucked: Vec<Option<usize>> = Vec::with_capacity(placed.len());
        for card in &placed {
            let by = card
                .holder
                .and_then(|holder| tucked[holder])
                .or_else(|| folds.get(&card.card.id).copied());
            tucked.push(by);
        }

        // Each card in sight where it shows now, on its way to where it goes, and how opaque: one
        // coming in comes out of the card holding it. Cards dragged are just where the mouse is.
        let snap = matches!(drag, Some(Drag::Cards(..)));
        let mut alphas = vec![1.; placed.len()];
        for ix in 0..placed.len() {
            if tucked[ix].is_some() {
                continue;
            }
            let from = placed[ix].holder.map(|hx| placed[hx].at);
            let card = &mut placed[ix];
            (card.at, alphas[ix]) = camera.place(&card.card.id, card.at, from, now, snap);
        }
        let rect = |ix: usize| {
            let card = &placed[ix];
            (card.at.0, card.at.1, card.size.0, card.size.1)
        };
        let index: HashMap<&SharedString, usize> = placed
            .iter()
            .enumerate()
            .map(|(ix, card)| (&card.card.id, ix))
            .collect();
        let holders: HashMap<SharedString, SharedString> = placed
            .iter()
            .filter_map(|card| Some((card.card.id.clone(), placed[card.holder?].card.id.clone())))
            .collect();
        let boxes = (0..placed.len())
            .filter(|ix| tucked[*ix].is_none() && placed[*ix].card.selectable)
            .map(|ix| (placed[ix].card.id.clone(), rect(ix)))
            .collect();

        // Each group around its cards in sight and all they hold, or in their place when
        // collapsed.
        let mut frames = Vec::new();
        let mut stands = Vec::new();
        let mut stood = vec![None; self.groups.len()];
        for (gx, group) in self.groups.into_iter().enumerate() {
            let want = group.collapsed.then_some(gx);
            let members: Vec<usize> = group
                .members
                .iter()
                .filter_map(|id| index.get(id).copied())
                .filter(|ix| tucked[*ix] == want)
                .collect();
            let wrapped = if group.collapsed {
                members.clone()
            } else {
                held(&placed, &members)
                    .into_iter()
                    .filter(|ix| tucked[*ix].is_none())
                    .collect()
            };
            let Some(hull) = hull(wrapped.iter().map(|ix| rect(*ix))) else {
                continue;
            };
            let keys: Vec<SharedString> = members
                .iter()
                .map(|ix| placed[*ix].card.id.clone())
                .collect();
            let movers = heads(&keys, &holders);
            if group.collapsed {
                let shown = camera.place(&group.id, (hull.0, hull.1), None, now, snap);
                stood[gx] = Some(shown);
                stands.push((group, shown, movers, keys.len()));
            } else {
                frames.push((group, frame(hull), movers, keys.len()));
            }
        }

        // A curve from a card's header to the header of each card it holds, lit to the open ones,
        // but none to a chip; one to a collapsed group from each card holding some of its cards.
        let mut edges = Vec::new();
        let mut joined = HashSet::new();
        for (ix, card) in placed.iter().enumerate() {
            let Some(hx) = card.holder.filter(|hx| tucked[*hx].is_none()) else {
                continue;
            };
            if card.card.form == Form::Chip {
                continue;
            }
            let holder = &placed[hx];
            let from = (holder.at.0 + holder.size.0, holder.at.1 + HEIGHT / 2.);
            match tucked[ix] {
                None => edges.push(Edge {
                    from,
                    to: (card.at.0, card.at.1 + HEIGHT / 2.),
                    stroke: if card.open { Stroke::Lit } else { Stroke::Tree },
                    alpha: alphas[ix],
                }),
                Some(gx) => {
                    if let Some((at, alpha)) = stood[gx]
                        && joined.insert((hx, gx))
                    {
                        edges.push(Edge {
                            from,
                            to: (at.0, at.1 + HEIGHT / 2.),
                            stroke: Stroke::Tree,
                            alpha,
                        });
                    }
                }
            }
        }
        camera.remember(self.nudges, boxes, holders, (layout.width, layout.height));
        if camera.settle(now) || panning {
            window.request_animation_frame();
        }

        // Only what is in sight is drawn: with many folders open, most of the graph is not.
        let sight = camera.in_sight();
        let seen = |rect: Rect| sight.is_none_or(|sight| touches(sight, rect));
        edges.retain(|edge| seen(reach(edge.from, edge.to)));

        let hands = Hands {
            camera: camera.clone(),
            group: self.on_group,
            hide: self.on_hide,
        };
        let mut cards = Vec::new();
        for ((mut card, out), alpha) in placed.into_iter().zip(tucked).zip(alphas) {
            if out.is_some() || !seen((card.at.0, card.at.1, card.size.0, card.size.1)) {
                continue;
            }
            let (x, y) = view.at(card.at);
            let height = match card.card.form {
                Form::Chip => {
                    cards.push(chip_element(card.card, (x, y), zoom, alpha, cx));
                    continue;
                }
                // The sheets go under the top of the pile, which is a card's height, and the chip
                // under them.
                Form::Pile => {
                    cards.extend(sheets((x, y), zoom, alpha, cx));
                    if let Some(Under { label, run }) = card.card.under.take() {
                        let chip = Card::new(format!("{}:under", card.card.id), label)
                            .icon(|size| Icon::new(Glyph::ChevronDown).size(size))
                            .on_click(move |window, cx| run(window, cx))
                            .chip();
                        let at = (x, y + (HEIGHT + SHEETS + UNDER) * zoom);
                        cards.push(chip_element(chip, at, zoom, alpha, cx));
                    }
                    HEIGHT
                }
                Form::Plain => card.size.1,
            };
            let frame = (x, y, card.size.0 * zoom, height * zoom);
            let look = Look {
                open: card.open,
                picked: camera.is_picked(&card.card.id),
                alpha,
            };
            cards.push(card_element(
                card.card,
                frame,
                look,
                Press::Card,
                zoom,
                &hands,
                cx,
            ));
        }
        for (group, (at, alpha), movers, count) in stands {
            if !seen((at.0, at.1, WIDTH, HEIGHT)) {
                continue;
            }
            let (x, y) = view.at(at);
            let frame = (x, y, WIDTH * zoom, HEIGHT * zoom);
            let card = group.stand_in(count);
            let look = Look {
                open: false,
                picked: false,
                alpha,
            };
            cards.push(card_element(
                card,
                frame,
                look,
                Press::Carry(movers),
                zoom,
                &hands,
                cx,
            ));
        }
        let (frames, titles): (Vec<AnyElement>, Vec<AnyElement>) = frames
            .into_iter()
            .map(|(group, rect, movers, count)| {
                frame_element(group, rect, movers, count, view, &camera, cx)
            })
            .unzip();
        let pick = match &drag {
            Some(Drag::Pick(from, to)) => Some(span(*from, *to)),
            _ => None,
        }
        .map(|(x, y, w, h)| {
            let (x, y) = view.at((x, y));
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w * zoom))
                .h(px(h * zoom))
                .rounded_sm()
                .border_1()
                .border_color(theme.blue)
                .bg(theme.blue.opacity(0.08))
        });
        let (width, height) = view.at((layout.width, layout.height));

        // The wheel moves the graph; with Ctrl it zooms around the mouse.
        let wheel = {
            let camera = camera.clone();
            move |event: &gpui_kit::ScrollEvent, window: &mut Window, _: &mut App| {
                let delta = event.delta.pixel_delta(px(16.));
                let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                if event.modifiers.control || event.modifiers.platform {
                    let at = camera.local(event.position);
                    camera.zoom_to(camera.zoom() * 1.0025_f32.powf(dy), at);
                } else {
                    camera.move_by((dx, dy));
                }
                window.refresh();
            }
        };
        let (ox, oy) = camera.offset();
        let content = div()
            .absolute()
            .left(px(ox))
            .top(px(oy))
            .w(px(width))
            .h(px(height))
            .children(frames)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| paint(edges, view, bounds, line, lit, window),
                )
                .absolute()
                .size_full(),
            )
            .children(cards)
            .children(titles)
            .children(pick);

        let untangle = self.on_reorganize.filter(|_| !nudges.is_empty());
        let (down, moved, up, framed) = (
            camera.clone(),
            camera.clone(),
            camera.clone(),
            camera.clone(),
        );
        let (on_move, on_drop) = (self.on_move.clone(), self.on_move);
        // With Space held, a sheet over the cards takes the press, so a drag anywhere moves the
        // view, the hand closing while it does.
        let hand = camera.hand().then(|| {
            let grip = camera.clone();
            let cursor = match drag {
                Some(Drag::View(..)) => CursorStyle::ClosedHand,
                _ => CursorStyle::OpenHand,
            };
            div()
                .id("hand")
                .absolute()
                .size_full()
                .cursor(cursor)
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    grip.press(event.position, false);
                    window.refresh();
                })
        });
        div()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.background)
            .when_some(self.focus, |el, focus| keys(el, focus, &camera))
            .on_scroll(wheel)
            .on_mouse_down(MouseButton::Left, move |event, window, _| {
                let pick = !down.hand();
                down.press(event.position, pick);
                window.refresh();
            })
            .on_mouse_move(move |event, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    if moved.follow(event.position) {
                        window.refresh();
                    }
                    return;
                }
                // Let go outside the graph.
                if let Some(moves) = moved.release() {
                    if let Some(f) = &on_move {
                        f(moves, window, cx);
                    }
                    window.refresh();
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                if let Some(moves) = up.release()
                    && let Some(f) = &on_drop
                {
                    f(moves, window, cx);
                }
                window.refresh();
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let (moved, resized) = framed.frame(bounds);
                        if moved {
                            window.refresh();
                        }
                        // Drawn again for the cards a viewport of another size brings in sight.
                        if resized {
                            window.request_animation_frame();
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(Scaled {
                scale: zoom,
                child: content.into_any_element(),
            })
            .children(hand)
            .child(zoom_bar(&camera, untangle, cx))
    }
}

// The graph takes the keys on a press, unless something inside it has them, and Space held makes
// a drag move the view.
fn keys(el: Stateful<Div>, focus: FocusHandle, camera: &Camera) -> Stateful<Div> {
    let space = |key: &str| key == "space";
    let (down, up) = (camera.clone(), camera.clone());
    let (take, own) = (focus.clone(), focus.clone());
    el.track_focus(&focus)
        .capture_any_mouse_down(move |_, window, cx| {
            if !take.contains_focused(window, cx) {
                take.focus(window, cx);
            }
        })
        // A Space typed in a field inside the graph, a group's title, stays a space.
        .on_key_down(move |event, window, cx| {
            let held = event.keystroke.modifiers;
            let plain = !(held.control || held.platform || held.alt);
            if !own.is_focused(window) || !space(&event.keystroke.key) || !plain {
                return;
            }
            cx.stop_propagation();
            if down.hold(true) {
                window.refresh();
            }
        })
        .on_key_up(move |event, window, _| {
            if space(&event.keystroke.key) && up.hold(false) {
                window.refresh();
            }
        })
}

fn zoom_bar(camera: &Camera, untangle: Option<Handler>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let button = |id: &'static str, glyph: Option<Glyph>, label: Option<String>| {
        div()
            .id(id)
            .h_6()
            .min_w_6()
            .px_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .text_xs()
            .text_color(theme.muted_foreground)
            .hover(|style| style.bg(theme.list_hover))
            .children(glyph.map(|glyph| Icon::new(glyph).small()))
            .children(label)
    };
    let act = |run: fn(&Camera)| {
        let camera = camera.clone();
        move |_: &gpui_kit::ClickEvent, window: &mut Window, _: &mut App| {
            run(&camera);
            window.refresh();
        }
    };
    let percent = format!("{:.0}%", camera.zoom() * 100.);
    // Back to the layout, when cards were moved by hand.
    let untangle = untangle.map(|run| {
        button("untangle", Some(Glyph::LayoutDashboard), None)
            .on_click(move |_, window, cx| run(window, cx))
    });
    h_flex()
        .id("zoom")
        .occlude()
        .absolute()
        .left_3()
        .top_3()
        .p_0p5()
        .gap_0p5()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .children(untangle)
        .child(
            button("zoom-out", Some(Glyph::Minus), None)
                .on_click(act(|camera| camera.glide_zoom(1. / STEP))),
        )
        .child(button("zoom-reset", None, Some(percent)).on_click(act(Camera::reset)))
        .child(
            button("zoom-in", Some(Glyph::Plus), None)
                .on_click(act(|camera| camera.glide_zoom(STEP))),
        )
}

fn shape(branch: &Branch) -> Shape {
    Shape {
        size: branch.card.size(),
        open: branch.open,
        apart: branch.apart,
        trails: branch.card.form == Form::Chip,
        children: branch.children.iter().map(shape).collect(),
    }
}

// The given cards and every card they hold, however deep.
fn held(placed: &[Placed], cards: &[usize]) -> Vec<usize> {
    let mut inside = vec![false; placed.len()];
    for (ix, card) in placed.iter().enumerate() {
        inside[ix] = cards.contains(&ix) || card.holder.is_some_and(|holder| inside[holder]);
    }
    (0..placed.len()).filter(|ix| inside[*ix]).collect()
}

// A card where it goes, with its size, and the card that holds it.
struct Placed {
    card: Card,
    open: bool,
    at: (f32, f32),
    size: (f32, f32),
    holder: Option<usize>,
}

// Puts each card where the layout says, moved by its own nudge and those of the cards that hold
// it, in the order the shape walks them, so a card comes before those it holds.
struct Placing<'a> {
    nudges: &'a HashMap<SharedString, (f32, f32)>,
    placed: &'a mut Vec<Placed>,
}

impl Placing<'_> {
    fn lay(
        &mut self,
        branch: Branch,
        shape: &Shape,
        spot: &Spot,
        carry: (f32, f32),
        holder: Option<usize>,
    ) {
        let own = self
            .nudges
            .get(&branch.card.id)
            .copied()
            .unwrap_or_default();
        let shift = (carry.0 + own.0, carry.1 + own.1);
        let ix = self.placed.len();
        self.placed.push(Placed {
            card: branch.card,
            open: branch.open,
            at: (spot.x + shift.0, spot.y + shift.1),
            size: shape.size,
            holder,
        });
        let kids = branch
            .children
            .into_iter()
            .zip(&shape.children)
            .zip(&spot.children);
        for ((child, inner), place) in kids {
            self.lay(child, inner, place, shift, Some(ix));
        }
    }
}

fn paint(
    mut edges: Vec<Edge>,
    view: View,
    bounds: Bounds<Pixels>,
    line: Hsla,
    lit: Hsla,
    window: &mut Window,
) {
    edges.sort_by_key(|edge| edge.stroke);
    let at = |p: (f32, f32)| {
        let (x, y) = view.at(p);
        point(bounds.origin.x + px(x), bounds.origin.y + px(y))
    };
    for edge in edges {
        let (from, to) = (edge.from, edge.to);
        let (a, b) = bends(from, to);
        let (width, color) = match edge.stroke {
            Stroke::Tree => (1., line),
            Stroke::Lit => (1.5, lit),
        };
        let mut path = PathBuilder::stroke(px(width * view.zoom.max(0.6)));
        path.move_to(at(from));
        path.cubic_bezier_to(at(to), at(a), at(b));
        if let Ok(path) = path.build() {
            window.paint_path(path, color.opacity(edge.alpha));
        }
    }
}

// What the cards' menus and presses reach: the camera, and what picked cards can be asked for.
struct Hands {
    camera: Camera,
    group: Option<Keys>,
    hide: Option<Keys>,
}

// What a press on a card's header takes along: the card, or a group's cards.
enum Press {
    Card,
    Carry(Vec<SharedString>),
}

struct Look {
    open: bool,
    picked: bool,
    // Fading in as it comes.
    alpha: f32,
}

fn card_element(
    card: Card,
    (x, y, w, h): (f32, f32, f32, f32),
    look: Look,
    press: Press,
    zoom: f32,
    hands: &Hands,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let title = match card.tint {
        _ if card.muted => theme.muted_foreground,
        Some((color, _)) => color,
        None => theme.foreground,
    };
    let head_id = ElementId::Name(format!("{}:head", card.id).into());
    let pile = card.form == Form::Pile;
    let clickable = card.on_click.is_some();
    let selectable = card.selectable;
    let grab = (hands.camera.clone(), card.id.clone());
    let head = h_flex()
        .id(head_id)
        // The topmost card under the mouse takes the press, not those under it.
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            cx.stop_propagation();
            let (camera, id) = &grab;
            match &press {
                Press::Card => {
                    let held = event.modifiers;
                    let toggle = selectable && (held.control || held.platform || held.shift);
                    camera.grab(id, event.position, toggle);
                }
                Press::Carry(keys) => camera.carry(keys, event.position),
            }
            window.refresh();
        })
        .flex_shrink_0()
        .h(px(HEIGHT * zoom))
        .px_3()
        .gap_2()
        .children(card.icon.map(|draw| draw(px(ICON * zoom))))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(title)
                        .child(card.title),
                )
                .children(
                    card.meta
                        .map(|meta| div().truncate().text_xs().text_color(muted).child(meta)),
                ),
        )
        .children(
            card.badge
                .map(|badge| Tag::secondary().xsmall().child(badge)),
        )
        .when_some(card.on_click, |el, f| {
            let camera = hands.camera.clone();
            el.cursor_pointer().on_click(move |_, window, cx| {
                if !camera.spent() {
                    f(window, cx);
                }
            })
        });
    // Picked with others, the card's menu is theirs: group them or hide them.
    let together = selectable && (hands.group.is_some() || hands.hide.is_some());
    let head = if card.actions.is_empty() && !together {
        head.into_any_element()
    } else {
        let actions = Rc::new(card.actions);
        let (camera, id) = (hands.camera.clone(), card.id.clone());
        let (group, hide) = (hands.group.clone(), hands.hide.clone());
        head.context_menu(move |menu, _, _| match camera.picked_with(&id) {
            Some(keys) if together => {
                let menu = pick_item(menu, "Group…", Glyph::Group, &keys, &camera, &group);
                pick_item(menu, "Hide selected", Glyph::EyeOff, &keys, &camera, &hide)
            }
            _ => fill(menu, &actions),
        })
        .into_any_element()
    };
    // A drag or a click inside the panel stays there: it neither moves the graph nor folds
    // the card.
    let drop = card.drop.map(|drop| {
        div()
            .id(ElementId::Name(format!("{}:drop", card.id).into()))
            .flex_1()
            .min_h_0()
            .border_t_1()
            .border_color(theme.border)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(drop.element)
    });
    let turn = card.on_turn.map(|f| {
        let (camera, id) = (hands.camera.clone(), card.id.clone());
        move |event: &gpui_kit::ScrollEvent, window: &mut Window, cx: &mut App| {
            let delta = event.delta.pixel_delta(px(16.));
            let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
            if event.modifiers.control || event.modifiers.platform || dx.abs() > dy.abs() {
                return;
            }
            cx.stop_propagation();
            let turns = camera.turns(&id, dy);
            if turns != 0 {
                f(turns, window, cx);
            }
        }
    });
    let bg = match card.tint {
        _ if look.picked => theme.sidebar.blend(theme.blue.opacity(0.14)),
        Some((color, true)) => theme.sidebar.blend(color.opacity(0.1)),
        _ => theme.sidebar,
    };
    v_flex()
        .id(ElementId::Name(card.id))
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .overflow_hidden()
        .opacity(look.alpha)
        .rounded_lg()
        .border_1()
        .border_color(if look.open || look.picked {
            theme.blue
        } else {
            theme.border
        })
        .bg(bg)
        .when(clickable && !look.open && !look.picked, |el| {
            el.hover(move |style| style.border_color(muted))
        })
        .when(pile, |el| el.shadow_md())
        .when_some(turn, |el, turn| el.on_scroll(turn))
        .child(head)
        .children(drop)
        .into_any_element()
}

// The two sheets showing under the top of a pile, each a little lighter than the card, the
// lower one narrower.
fn sheets((x, y): (f32, f32), zoom: f32, alpha: f32, cx: &App) -> [AnyElement; 2] {
    let theme = cx.theme();
    let sheet = |down: f32, inset: f32, bg: Hsla| {
        div()
            .absolute()
            .left(px(x + inset * zoom))
            .top(px(y + down * zoom))
            .w(px((WIDTH - 2. * inset) * zoom))
            .h(px(HEIGHT * zoom))
            .opacity(alpha)
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(bg)
            .into_any_element()
    };
    let lift = |by: f32| theme.sidebar.blend(theme.foreground.opacity(by));
    [
        sheet(SHEETS, 12., lift(0.02)),
        sheet(SHEETS / 2., 6., lift(0.05)),
    ]
}

// A chip at the end of a column: its icon and title in a dashed pill. A press on it neither
// moves the graph nor picks anything.
fn chip_element(card: Card, (x, y): (f32, f32), zoom: f32, alpha: f32, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (muted, fg, hover) = (theme.muted_foreground, theme.foreground, theme.sidebar);
    h_flex()
        .id(ElementId::Name(card.id))
        .absolute()
        .left(px(x))
        .top(px(y))
        .h(px(CHIP * zoom))
        .opacity(alpha)
        .px_3()
        .gap_1p5()
        .rounded_full()
        .border_1()
        .border_dashed()
        .border_color(theme.border)
        .text_xs()
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(fg).border_color(muted).bg(hover))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(card.icon.map(|draw| draw(px(12. * zoom))))
        .child(card.title)
        .when_some(card.on_click, |el, f| {
            el.on_click(move |_, window, cx| f(window, cx))
        })
        .into_any_element()
}

// An open group: a light fill behind its cards, and over them its title, which drags its cards
// and opens its menu, so no card hides it.
fn frame_element(
    group: Group,
    rect: Rect,
    movers: Vec<SharedString>,
    count: usize,
    view: View,
    camera: &Camera,
    cx: &App,
) -> (AnyElement, AnyElement) {
    let theme = cx.theme();
    let zoom = view.zoom;
    let (x, y) = view.at((rect.0, rect.1));
    let (w, h) = (rect.2 * zoom, rect.3 * zoom);
    let body = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .rounded_lg()
        .border_1()
        .border_color(theme.blue.opacity(0.45))
        .bg(theme.blue.opacity(0.05))
        .into_any_element();
    let title = match group.editor {
        // Typing in the title neither drags the group nor moves the graph.
        Some(editor) => div()
            .w(px((WIDTH - 2. * RIM) * zoom))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(editor)
            .into_any_element(),
        None => h_flex()
            .min_w_0()
            .gap_1p5()
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(group.title),
            )
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(count.to_string()),
            )
            .into_any_element(),
    };
    let carry = camera.clone();
    let actions = Rc::new(group.actions);
    let head = h_flex()
        .id(ElementId::Name(format!("{}:title", group.id).into()))
        .absolute()
        .left(px(x))
        .top(px(y))
        .max_w(px(w))
        .h(px(TITLE * zoom))
        .px_2()
        .gap_1p5()
        .rounded_lg()
        .border_1()
        .border_color(theme.blue.opacity(0.45))
        .bg(theme.sidebar.blend(theme.blue.opacity(0.18)))
        .text_xs()
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            cx.stop_propagation();
            carry.carry(&movers, event.position);
            window.refresh();
        })
        .child(
            Icon::new(Glyph::Group)
                .size(px(12. * zoom))
                .text_color(theme.blue),
        )
        .child(title)
        .context_menu(move |menu, _, _| fill(menu, &actions))
        .into_any_element();
    (body, head)
}

fn fill(mut menu: PopupMenu, actions: &[Action]) -> PopupMenu {
    for action in actions {
        let run = action.run.clone();
        let item = PopupMenuItem::new(action.label.clone())
            .icon(action.glyph)
            .on_click(move |_, window, cx| run(window, cx));
        menu = menu.item(item);
    }
    menu
}

// An entry of the menu of picked cards, when the graph's owner answers it.
fn pick_item(
    menu: PopupMenu,
    label: &'static str,
    glyph: Glyph,
    keys: &[SharedString],
    camera: &Camera,
    run: &Option<Keys>,
) -> PopupMenu {
    let Some(run) = run.clone() else {
        return menu;
    };
    let (keys, camera) = (keys.to_vec(), camera.clone());
    menu.item(
        PopupMenuItem::new(label)
            .icon(glyph)
            .on_click(move |_, window, cx| {
                camera.unpick();
                run(keys.clone(), window, cx);
            }),
    )
}
