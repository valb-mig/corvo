use std::cell::RefCell;
use std::collections::HashMap;
use std::panic;
use std::rc::Rc;

use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseButton,
    ParentElement as _, PathBuilder, Pixels, Point, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point,
    prelude::FluentBuilder as _, px,
};

// Cards as a tree read left to right: an open card holds its cards in the next column, with a
// curve to each, and any number of cards can be open at once. A card can drop a panel down from
// under its header; its column widens to hold it.

pub const WIDTH: f32 = 232.;
pub const HEIGHT: f32 = 52.;
pub const GAP: f32 = 10.;
pub const SPAN: f32 = 72.;
pub const PAD: f32 = 24.;
pub const ZOOM_MIN: f32 = 0.3;
pub const ZOOM_MAX: f32 = 2.;

const ICON: f32 = 16.;
const STEP: f32 = 1.2;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type Draw = Box<dyn Fn(Pixels) -> AnyElement>;

pub struct Card {
    id: SharedString,
    icon: Option<Draw>,
    title: SharedString,
    meta: Option<SharedString>,
    badge: Option<SharedString>,
    muted: bool,
    on_click: Option<Handler>,
    actions: Vec<Action>,
    drop: Option<Drop>,
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
            on_click: None,
            actions: Vec::new(),
            drop: None,
        }
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

    fn size(&self) -> (f32, f32) {
        match &self.drop {
            Some(drop) => (drop.width.max(WIDTH), HEIGHT + drop.height),
            None => (WIDTH, HEIGHT),
        }
    }
}

// A card and, when it is open, the cards it holds.
pub struct Branch {
    card: Card,
    open: bool,
    children: Vec<Branch>,
}

impl Branch {
    pub fn new(card: Card) -> Self {
        Self {
            card,
            open: false,
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Branch>) -> Self {
        self.children.extend(children);
        self
    }
}

// A branch as the size of its card, whether it is open, and what it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub size: (f32, f32),
    pub open: bool,
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
    pub root: Spot,
    pub width: f32,
    pub height: f32,
}

// Every depth is a column as wide as its widest card. A card sits halfway between the first and
// the last card it holds, and each branch takes the height of all it holds, so the cards around it
// make room and nothing overlaps however much is open.
pub fn layout(root: &Shape) -> Layout {
    let mut widths = Vec::new();
    measure(root, 0, &mut widths);
    let mut lefts = Vec::with_capacity(widths.len());
    let mut x = PAD;
    for width in &widths {
        lefts.push(x);
        x += width + SPAN;
    }
    let (root, bottom) = place(root, 0, PAD, &lefts);
    Layout {
        root,
        width: x - SPAN + PAD,
        height: bottom + PAD,
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
    for child in &shape.children {
        let (spot, bottom) = place(child, depth + 1, low, lefts);
        children.push(spot);
        low = bottom + GAP;
    }
    // The card sits halfway between the first card it holds and the last.
    let y = (children[0].y + children[children.len() - 1].y) / 2.;
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

// Where a graph is looked at from, free of any edge: how far it is moved and how close. It lives
// with the graph's owner between renders.
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
    // The drag went far enough to be one, so the click it ends in is not a click.
    dragged: bool,
    // How far each card was moved by hand, in graph space; what it holds moves with it.
    nudges: HashMap<SharedString, (f32, f32)>,
    // The size of the graph at zoom 1, as last laid out.
    extent: (f32, f32),
    // Fit the graph to the viewport on the next paint, once its size is known.
    fit: bool,
}

// What a drag moves, where the mouse was pressed, and where that was before.
#[derive(Clone)]
enum Drag {
    View(Point<Pixels>, (f32, f32)),
    Card(SharedString, Point<Pixels>, (f32, f32)),
}

impl Default for Lens {
    fn default() -> Self {
        Self {
            zoom: 1.,
            offset: (0., 0.),
            bounds: Bounds::default(),
            drag: None,
            dragged: false,
            nudges: HashMap::new(),
            extent: (0., 0.),
            fit: false,
        }
    }
}

impl Camera {
    pub fn zoom(&self) -> f32 {
        self.lens.borrow().zoom
    }

    pub fn offset(&self) -> (f32, f32) {
        self.lens.borrow().offset
    }

    fn move_by(&self, (dx, dy): (f32, f32)) {
        let mut lens = self.lens.borrow_mut();
        lens.offset = (lens.offset.0 + dx, lens.offset.1 + dy);
    }

    // Zooms keeping the graph point under the viewport point `at` where it is.
    fn zoom_to(&self, zoom: f32, at: (f32, f32)) {
        let before = View::new(self.zoom());
        let zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let after = View::new(zoom);
        let p = before.under(at, self.offset());
        let offset = after.offset(p, at);
        let mut lens = self.lens.borrow_mut();
        lens.zoom = zoom;
        lens.offset = offset;
    }

    // Back to where it started: zoom 1, nothing moved.
    fn reset(&self) {
        let mut lens = self.lens.borrow_mut();
        lens.zoom = 1.;
        lens.offset = (0., 0.);
    }

    // A window point in the viewport.
    fn local(&self, at: Point<Pixels>) -> (f32, f32) {
        let at = at - self.lens.borrow().bounds.origin;
        (f32::from(at.x), f32::from(at.y))
    }

    fn dragged(&self) -> bool {
        self.lens.borrow().dragged
    }

    // A press on a card's header takes the card along when the mouse moves.
    fn grab(&self, id: SharedString, at: Point<Pixels>) {
        let mut lens = self.lens.borrow_mut();
        let from = lens.nudges.get(&id).copied().unwrap_or_default();
        lens.drag = Some(Drag::Card(id, at, from));
        lens.dragged = false;
    }

    // A press anywhere else moves the view, unless it was a card's.
    fn press(&self, at: Point<Pixels>) {
        let mut lens = self.lens.borrow_mut();
        if matches!(&lens.drag, Some(Drag::Card(_, start, _)) if *start == at) {
            return;
        }
        lens.drag = Some(Drag::View(at, lens.offset));
        lens.dragged = false;
    }

    // Follows the mouse with what was pressed; false once no drag goes on.
    fn follow(&self, at: Point<Pixels>, held: bool) -> bool {
        let mut lens = self.lens.borrow_mut();
        let Some(drag) = lens.drag.clone() else {
            return false;
        };
        if !held {
            lens.drag = None;
            return false;
        }
        let (start, from) = match &drag {
            Drag::View(start, from) | Drag::Card(_, start, from) => (*start, *from),
        };
        let delta = at - start;
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        if dx.abs() + dy.abs() > 4. {
            lens.dragged = true;
        }
        match drag {
            Drag::View(..) => lens.offset = (from.0 + dx, from.1 + dy),
            Drag::Card(id, ..) => {
                let zoom = lens.zoom;
                lens.nudges
                    .insert(id, (from.0 + dx / zoom, from.1 + dy / zoom));
            }
        }
        true
    }

    fn release(&self) {
        self.lens.borrow_mut().drag = None;
    }

    pub fn is_rearranged(&self) -> bool {
        !self.lens.borrow().nudges.is_empty()
    }

    // Every card back where the layout puts it, and the whole graph centred in view.
    pub fn reorganize(&self) {
        let mut lens = self.lens.borrow_mut();
        lens.nudges.clear();
        lens.fit = true;
    }

    // Records the viewport, fitting the graph to it when asked; true when the camera moved.
    fn frame(&self, bounds: Bounds<Pixels>) -> bool {
        let mut lens = self.lens.borrow_mut();
        lens.bounds = bounds;
        let viewport = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if !lens.fit || viewport.0 <= 0. || viewport.1 <= 0. {
            return false;
        }
        let (zoom, offset) = fit(lens.extent, viewport);
        lens.zoom = zoom;
        lens.offset = offset;
        lens.fit = false;
        true
    }

    fn middle(&self) -> (f32, f32) {
        let size = self.lens.borrow().bounds.size;
        (f32::from(size.width) / 2., f32::from(size.height) / 2.)
    }
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
}

#[derive(IntoElement)]
pub struct Graph {
    id: ElementId,
    camera: Camera,
    root: Branch,
}

impl Graph {
    pub fn new(id: impl Into<ElementId>, camera: &Camera, root: Branch) -> Self {
        Self {
            id: id.into(),
            camera: camera.clone(),
            root,
        }
    }
}

impl RenderOnce for Graph {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let shape = shape(&self.root);
        let layout = layout(&shape);
        let camera = self.camera;
        let view = View::new(camera.zoom());
        let zoom = view.zoom;
        let (line, lit) = (theme.muted_foreground.opacity(0.45), theme.blue);

        let mut edges = Vec::new();
        let mut flat = Vec::new();
        let nudges = camera.lens.borrow().nudges.clone();
        camera.lens.borrow_mut().extent = (layout.width, layout.height);
        let mut placing = Placing {
            nudges: &nudges,
            cards: &mut flat,
            edges: &mut edges,
        };
        placing.lay(self.root, &shape, &layout.root, (0., 0.));
        let mut cards = Vec::new();
        for (card, open, (x, y), (w, h)) in flat {
            let (x, y) = view.at((x, y));
            let frame = (x, y, w * zoom, h * zoom);
            cards.push(card_element(card, frame, open, zoom, &camera, cx));
        }
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
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| paint(edges, view, bounds, line, lit, window),
                )
                .absolute()
                .size_full(),
            )
            .children(cards);

        let (down, moved, up, seen) = (
            camera.clone(),
            camera.clone(),
            camera.clone(),
            camera.clone(),
        );
        div()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.background)
            .on_scroll(wheel)
            .on_mouse_down(MouseButton::Left, move |event, _, _| {
                down.press(event.position)
            })
            .on_mouse_move(move |event, window, _| {
                let held = event.pressed_button == Some(MouseButton::Left);
                if moved.follow(event.position, held) {
                    window.refresh();
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| up.release())
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if seen.frame(bounds) {
                            window.refresh();
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
            .child(zoom_bar(&camera, cx))
    }
}

fn zoom_bar(camera: &Camera, cx: &App) -> impl IntoElement {
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
    let untangle = camera.is_rearranged().then(|| {
        button("untangle", Some(Glyph::LayoutDashboard), None).on_click(act(Camera::reorganize))
    });
    h_flex()
        .id("zoom")
        .occlude()
        .absolute()
        .right_3()
        .bottom_3()
        .p_0p5()
        .gap_0p5()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .children(untangle)
        .child(
            button("zoom-out", Some(Glyph::Minus), None).on_click(act(|camera| {
                camera.zoom_to(camera.zoom() / STEP, camera.middle());
            })),
        )
        .child(button("zoom-reset", None, Some(percent)).on_click(act(Camera::reset)))
        .child(
            button("zoom-in", Some(Glyph::Plus), None).on_click(act(|camera| {
                camera.zoom_to(camera.zoom() * STEP, camera.middle());
            })),
        )
}

fn shape(branch: &Branch) -> Shape {
    Shape {
        size: branch.card.size(),
        open: branch.open,
        children: branch.children.iter().map(shape).collect(),
    }
}

// The cards in the order the shape walks them, each with where it goes and its size.
type Placed = (Card, bool, (f32, f32), (f32, f32));

// Puts each card where the layout says, moved by its own nudge and those of the cards that hold
// it, and draws a curve from a card's header to the header of each card it holds, lit to the
// open ones.
struct Placing<'a> {
    nudges: &'a HashMap<SharedString, (f32, f32)>,
    cards: &'a mut Vec<Placed>,
    edges: &'a mut Vec<Edge>,
}

impl Placing<'_> {
    fn lay(&mut self, branch: Branch, shape: &Shape, spot: &Spot, carry: (f32, f32)) -> (f32, f32) {
        let own = self
            .nudges
            .get(&branch.card.id)
            .copied()
            .unwrap_or_default();
        let shift = (carry.0 + own.0, carry.1 + own.1);
        let at = (spot.x + shift.0, spot.y + shift.1);
        let from = (at.0 + shape.size.0, at.1 + HEIGHT / 2.);
        self.cards.push((branch.card, branch.open, at, shape.size));
        let kids = branch
            .children
            .into_iter()
            .zip(&shape.children)
            .zip(&spot.children);
        for ((child, inner), place) in kids {
            let stroke = if inner.open {
                Stroke::Lit
            } else {
                Stroke::Tree
            };
            let to = self.lay(child, inner, place, shift);
            self.edges.push(Edge {
                from,
                to: (to.0, to.1 + HEIGHT / 2.),
                stroke,
            });
        }
        at
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
        let bend = ((to.0 - from.0) / 2.).max(SPAN / 2.);
        let (width, color) = match edge.stroke {
            Stroke::Tree => (1., line),
            Stroke::Lit => (1.5, lit),
        };
        let mut path = PathBuilder::stroke(px(width * view.zoom.max(0.6)));
        path.move_to(at(from));
        path.cubic_bezier_to(at(to), at((from.0 + bend, from.1)), at((to.0 - bend, to.1)));
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

fn card_element(
    card: Card,
    (x, y, w, h): (f32, f32, f32, f32),
    picked: bool,
    zoom: f32,
    camera: &Camera,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let title = if card.muted {
        theme.muted_foreground
    } else {
        theme.foreground
    };
    let head_id = ElementId::Name(format!("{}:head", card.id).into());
    let clickable = card.on_click.is_some();
    let grab = (camera.clone(), card.id.clone());
    let head = h_flex()
        .id(head_id)
        .on_mouse_down(MouseButton::Left, move |event, _, _| {
            grab.0.grab(grab.1.clone(), event.position);
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
            let camera = camera.clone();
            el.cursor_pointer().on_click(move |_, window, cx| {
                if !camera.dragged() {
                    f(window, cx);
                }
            })
        });
    let head = if card.actions.is_empty() {
        head.into_any_element()
    } else {
        let actions = Rc::new(card.actions);
        head.context_menu(move |mut menu, _, _| {
            for action in actions.iter() {
                let run = action.run.clone();
                let item = PopupMenuItem::new(action.label.clone())
                    .icon(action.glyph)
                    .on_click(move |_, window, cx| run(window, cx));
                menu = menu.item(item);
            }
            menu
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
    v_flex()
        .id(ElementId::Name(card.id))
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .overflow_hidden()
        .rounded_lg()
        .border_1()
        .border_color(if picked { theme.blue } else { theme.border })
        .bg(theme.sidebar)
        .when(clickable && !picked, |el| {
            el.hover(move |style| style.border_color(muted))
        })
        .child(head)
        .children(drop)
        .into_any_element()
}
