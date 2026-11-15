mod applets;
mod backdrop;
mod glass;
mod paint;
mod pet;
mod settings;
mod shooter;
mod sprites;
mod store;
mod tile;
mod viewer;
mod warp;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontFamily, FontId, Id, LayerId, Order, PointerButton, Pos2, Rect, Sense,
    TextStyle, Vec2,
};
use egui_glass::{Glass, GlassStyle, LiveBackdrop, LiveBackdropQuality};

use applets::{ClickSpeed, Clipboard, Kind, Notepad, Photo, Snake, SystemStats, Todo};
use backdrop::Wallpaper;
use paint::Paint;
use pet::{Input as PetInput, Pet, Platform, Sheet};
use settings::{Choice, Hub, Prefs, Settings};
use shooter::Shooter;
use store::{Saved, Store};
use tile::{Cell, Out, Tile, View};
use viewer::Viewer;
use warp::Warp;

const CELL: f32 = 112.0;
const MARGIN: f32 = 24.0;
const STIFFNESS: f32 = 260.0;
const DAMPING_RATIO: f32 = 0.82;

struct Grid {
    origin: Pos2,
    cols: usize,
    rows: usize,
    gap: f32,
}

impl Grid {
    fn new(avail: Rect, gap: f32) -> Self {
        let fit = |len: f32| (((len - 2.0 * MARGIN + gap) / (CELL + gap)).floor() as usize).max(1);
        let (cols, rows) = (fit(avail.width()), fit(avail.height()));
        let size = vec2(cols as f32, rows as f32) * (CELL + gap) - vec2(gap, gap);
        Self {
            origin: avail.center() - size / 2.0,
            cols,
            rows,
            gap,
        }
    }

    fn rect(&self, (c, r, w, h): Cell) -> Rect {
        let min = self.origin + vec2(c as f32, r as f32) * (CELL + self.gap);
        let size = vec2(w as f32, h as f32) * CELL + vec2(w as f32 - 1.0, h as f32 - 1.0) * self.gap;
        Rect::from_min_size(min, size)
    }
}

fn overlaps(a: Cell, b: Cell) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

#[derive(Clone, Copy)]
struct Motion {
    p: [f32; 4],
    v: [f32; 4],
}

impl Motion {
    fn at(r: Rect) -> Self {
        Self { p: [r.min.x, r.min.y, r.max.x, r.max.y], v: [0.0; 4] }
    }

    fn rect(&self) -> Rect {
        Rect::from_min_max(pos2(self.p[0], self.p[1]), pos2(self.p[2], self.p[3]))
    }

    fn step(&mut self, target: Rect, dt: f32, direct: bool) -> bool {
        let t = [target.min.x, target.min.y, target.max.x, target.max.y];
        if direct {
            *self = Self::at(target);
            return false;
        }
        let k = STIFFNESS;
        let c = 2.0 * DAMPING_RATIO * k.sqrt();
        let steps = (dt / (1.0 / 240.0)).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            for i in 0..4 {
                self.v[i] += (k * (t[i] - self.p[i]) - c * self.v[i]) * h;
                self.p[i] += self.v[i] * h;
            }
        }
        (0..4).any(|i| (t[i] - self.p[i]).abs() > 0.05 || self.v[i].abs() > 0.5)
    }
}

struct Slot {
    id: u64,
    rect: Rect,
    glass: Rect,
    style: GlassStyle,
    focused: bool,
    dragged: bool,
    resized: bool,
}

struct App {
    live: LiveBackdrop,
    wallpaper: Wallpaper,
    hub: Hub,
    style: GlassStyle,
    tiles: Vec<Tile>,
    motion: HashMap<u64, Motion>,
    closing: HashSet<u64>,
    next_id: u64,
    cols: usize,
    rows: usize,
    focus: Option<u64>,
    expanded: bool,
    editing: bool,
    drag: Option<(u64, Vec2)>,
    resize: Option<(u64, Vec2)>,
    warp: Warp,
    hold: Option<(Pos2, f64)>,
    pet: Pet,
    sheet: Sheet,
    store: Store,
    last_save: f64,
}

fn default_tiles(hub: &Hub) -> Vec<Tile> {
    vec![
        Tile::new(1, (0, 0, 4, 1), vec![Box::new(SystemStats::new(hub.clone()))]),
        Tile::new(2, (0, 1, 2, 2), vec![Box::new(Todo::new()), Box::new(Notepad::new())]),
        Tile::new(3, (2, 1, 2, 2), vec![Box::new(Clipboard::new(hub.clone())), Box::new(Paint::new())]),
        Tile::new(4, (4, 0, 2, 2), vec![Box::new(Photo::new()), Box::new(Viewer::new())]),
        Tile::new(
            5,
            (4, 2, 2, 2),
            vec![
                Box::new(ClickSpeed::new()),
                Box::new(Snake::new(hub.clone())),
                Box::new(Shooter::new(hub.clone())),
            ],
        ),
        Tile::new(6, (0, 3, 4, 1), vec![Box::new(Settings::new(hub.clone()))]),
    ]
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc.wgpu_render_state.clone().expect("eframe must use the wgpu renderer");
        egui_glass::init(&rs, 1);

        let fonts = font_definitions();
        cc.egui_ctx.set_fonts(fonts.clone());
        let live = LiveBackdrop::new(&rs, Some(fonts));
        let mut wallpaper = Wallpaper::new(&rs);
        setup_style(&cc.egui_ctx);

        let mut store = Store::open();
        let saved = store.load();
        let hub = settings::hub(saved.as_ref().map_or_else(Prefs::default, |s| s.prefs.clone()));

        let wanted = std::env::args().nth(1).map(|a| PathBuf::from(a.trim().trim_matches('"')));
        let wanted = wanted.or_else(|| hub.borrow().wallpaper_path.clone());
        if let Some(path) = wanted {
            match wallpaper.load(&path) {
                Ok(()) => hub.borrow_mut().wallpaper_path = Some(path),
                Err(e) => hub.borrow_mut().error = Some(e),
            }
        }
        hub.borrow_mut().label = wallpaper.label().to_string();

        let (mut tiles, next_id) = match &saved {
            Some(s) => (
                s.tiles.iter().filter_map(|t| Tile::restore(t, &hub, &cc.egui_ctx, store.dir())).collect(),
                s.next_id,
            ),
            None => (default_tiles(&hub), 6),
        };
        if tiles.is_empty() && saved.as_ref().is_some_and(|s| !s.tiles.is_empty()) {
            tiles = default_tiles(&hub);
        }
        let sheet = Sheet::load(&cc.egui_ctx);

        Self {
            live,
            wallpaper,
            hub,
            style: glass_style(),
            tiles,
            motion: HashMap::new(),
            closing: HashSet::new(),
            next_id,
            cols: 6,
            rows: 4,
            focus: None,
            expanded: false,
            editing: false,
            drag: None,
            resize: None,
            warp: Warp::new(),
            hold: None,
            pet: Pet::new(),
            sheet,
            store,
            last_save: 0.0,
        }
    }

    fn sync_prefs(&mut self) {
        let hub = self.hub.clone();
        let mut p = hub.borrow_mut();
        match p.request.take() {
            Some(Choice::Default) => {
                self.wallpaper.use_default();
                p.wallpaper_path = None;
                p.error = None;
            }
            Some(Choice::File(path)) => match self.wallpaper.load(&path) {
                Ok(()) => {
                    p.wallpaper_path = Some(path);
                    p.error = None;
                }
                Err(e) => p.error = Some(e),
            },
            None => {}
        }
        if let Some(e) = self.wallpaper.take_error() {
            p.error = Some(e);
        }
        p.label = self.wallpaper.label().to_string();
        let reset = std::mem::take(&mut p.reset_layout);
        drop(p);
        if reset {
            self.tiles = default_tiles(&self.hub);
            self.next_id = 6;
            self.closing.clear();
            self.focus = None;
            self.expanded = false;
            self.end_edit();
        }
    }

    fn snapshot(&mut self) -> Saved {
        let dir = self.store.dir().to_path_buf();
        let closing = &self.closing;
        Saved {
            prefs: self.hub.borrow().clone(),
            tiles: self.tiles.iter_mut().filter(|t| !closing.contains(&t.id)).map(|t| t.snapshot(&dir)).collect(),
            next_id: self.next_id,
        }
    }

    fn save_now(&mut self) {
        let saved = self.snapshot();
        self.store.save(&saved);
    }

    fn is_free(&self, skip: Option<u64>, cell: Cell) -> bool {
        cell.0 + cell.2 <= self.cols
            && cell.1 + cell.3 <= self.rows
            && self
                .tiles
                .iter()
                .filter(|t| Some(t.id) != skip && !self.closing.contains(&t.id))
                .all(|t| !overlaps(t.cell, cell))
    }

    fn find_spot(&self, skip: Option<u64>, spans: &[(usize, usize)]) -> Option<Cell> {
        for &(w, h) in spans {
            if w > self.cols || h > self.rows {
                continue;
            }
            for r in 0..=self.rows - h {
                for c in 0..=self.cols - w {
                    let cell = (c, r, w, h);
                    if self.is_free(skip, cell) {
                        return Some(cell);
                    }
                }
            }
        }
        None
    }

    fn add_tile(&mut self, kind: Kind, grid: &Grid) {
        let span = kind.span();
        if let Some(cell) = self.find_spot(None, &[span, (2, 1), (1, 1)]) {
            self.next_id += 1;
            let seed = Rect::from_center_size(grid.rect(cell).center(), vec2(6.0, 6.0));
            self.motion.insert(self.next_id, Motion::at(seed));
            self.tiles.push(Tile::new(self.next_id, cell, vec![kind.build(&self.hub)]));
        }
    }

    fn set_cell(&mut self, id: u64, cell: Cell) {
        if self.is_free(Some(id), cell) {
            if let Some(t) = self.tiles.iter_mut().find(|t| t.id == id) {
                t.cell = cell;
            }
        }
    }

    fn relocate(&mut self) {
        let outside: Vec<(u64, usize, usize)> = self
            .tiles
            .iter()
            .filter(|t| t.cell.0 + t.cell.2 > self.cols || t.cell.1 + t.cell.3 > self.rows)
            .map(|t| (t.id, t.cell.2, t.cell.3))
            .collect();
        for (id, w, h) in outside {
            if let Some(cell) = self.find_spot(Some(id), &[(w, h)]) {
                self.set_cell(id, cell);
            }
        }
    }

    fn cell_of(&self, id: u64) -> Option<Cell> {
        self.tiles.iter().find(|t| t.id == id).map(|t| t.cell)
    }

    fn finish_drag(&mut self, grid: &Grid, id: u64) {
        let Some((_, d)) = self.drag.take() else { return };
        let Some(cell) = self.cell_of(id) else { return };
        let min = grid.rect(cell).translate(d).min;
        let step = CELL + grid.gap;
        let col = (((min.x - grid.origin.x) / step).round() as i32).clamp(0, self.cols.saturating_sub(cell.2) as i32);
        let row = (((min.y - grid.origin.y) / step).round() as i32).clamp(0, self.rows.saturating_sub(cell.3) as i32);
        self.set_cell(id, (col as usize, row as usize, cell.2, cell.3));
    }

    fn finish_resize(&mut self, grid: &Grid, id: u64) {
        let Some((_, d)) = self.resize.take() else { return };
        let Some(cell) = self.cell_of(id) else { return };
        let size = grid.rect(cell).size() + d;
        let step = CELL + grid.gap;
        let w = (((size.x + grid.gap) / step).round() as i32).clamp(1, self.cols.saturating_sub(cell.0).max(1) as i32);
        let h = (((size.y + grid.gap) / step).round() as i32).clamp(1, self.rows.saturating_sub(cell.1).max(1) as i32);
        self.set_cell(id, (cell.0, cell.1, w as usize, h as usize));
    }

    fn end_edit(&mut self) {
        self.editing = false;
        self.drag = None;
        self.resize = None;
    }

    fn platforms(&self, slots: &[Slot], bounds: Rect) -> Vec<Platform> {
        let mut out = vec![Platform { id: 0, x0: bounds.min.x + 8.0, x1: bounds.max.x - 8.0, y: bounds.max.y - 6.0 }];
        let live: Vec<&Slot> = slots
            .iter()
            .filter(|s| !self.closing.contains(&s.id) && (self.focus.is_none() || s.focused))
            .collect();
        for s in &live {
            let y = s.rect.min.y;
            let mut spans = vec![(s.rect.min.x + 20.0, s.rect.max.x - 20.0)];
            for o in live.iter().filter(|o| o.id != s.id && o.rect.min.y < y && o.rect.max.y > y) {
                spans = spans
                    .into_iter()
                    .flat_map(|(x0, x1)| {
                        if o.rect.max.x <= x0 || o.rect.min.x >= x1 {
                            return vec![(x0, x1)];
                        }
                        let mut v = Vec::new();
                        if o.rect.min.x > x0 {
                            v.push((x0, o.rect.min.x));
                        }
                        if o.rect.max.x < x1 {
                            v.push((o.rect.max.x, x1));
                        }
                        v
                    })
                    .collect();
            }
            for (k, (x0, x1)) in spans.into_iter().enumerate() {
                if x1 - x0 >= 36.0 {
                    out.push(Platform { id: s.id * 16 + 1 + k as u64, x0, x1, y });
                }
            }
        }
        out
    }
}

fn glass_style() -> GlassStyle {
    GlassStyle {
        corner_radius: 40.0,
        corner_smoothing: 1.0,
        blur: 2.0,
        refraction: 60.0,
        edge_width: 50.0,
        chromatic: 0.3,
        tint: Color32::from_rgba_unmultiplied(255, 255, 255, 0),
        brightness: 1.0,
        saturation: 1.2,
        specular: 0.9,
        border: 0.5,
        shadow: 0.0,
        zoom: 1.08,
        ..GlassStyle::clear()
    }
}

fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "inter_bold".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Bold.ttf")).into(),
    );
    fonts.font_data.insert(
        "jbmono".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMonoNF-Regular.ttf")).into(),
    );

    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".into());

    let mut bold = fonts.families[&FontFamily::Proportional].clone();
    bold[0] = "inter_bold".into();
    fonts.families.insert(FontFamily::Name("InterBold".into()), bold);
    fonts
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(visuals());
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Name("InterBold".into()))),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = vec2(6.0, 5.0);
        s.spacing.button_padding = vec2(9.0, 4.0);
    });
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(Color32::WHITE);
    v.extreme_bg_color = Color32::from_white_alpha(20);
    v.selection.bg_fill = Color32::from_white_alpha(70);
    let radius = egui::CornerRadius::same(12);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_white_alpha(22);
    v.widgets.inactive.bg_fill = Color32::from_white_alpha(22);
    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = Color32::from_white_alpha(44);
    v.widgets.hovered.bg_fill = Color32::from_white_alpha(44);
    v.widgets.active.weak_bg_fill = Color32::from_white_alpha(66);
    v.widgets.active.bg_fill = Color32::from_white_alpha(66);
    v
}

fn mod_held(m: &egui::Modifiers, mode: usize) -> bool {
    match mode {
        1 => m.ctrl && m.shift,
        2 => m.shift && !m.ctrl,
        3 => m.alt,
        _ => m.ctrl && !m.shift,
    }
}

fn wheel_amount(ctx: &egui::Context, mode: usize) -> f32 {
    ctx.input(|i| {
        i.events
            .iter()
            .map(|e| match e {
                egui::Event::MouseWheel { unit, delta, modifiers, .. } if mod_held(modifiers, mode) => {
                    let d = delta.x + delta.y;
                    match unit {
                        egui::MouseWheelUnit::Point => d / 40.0,
                        _ => d,
                    }
                }
                _ => 0.0,
            })
            .sum()
    })
}

fn distance_to(r: Rect, p: Pos2) -> f32 {
    let dx = (r.min.x - p.x).max(0.0).max(p.x - r.max.x);
    let dy = (r.min.y - p.y).max(0.0).max(p.y - r.max.y);
    dx.hypot(dy)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.sync_prefs();

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.editing {
                self.end_edit();
            } else {
                self.expanded = false;
            }
        }

        let prefs = self.hub.borrow().clone();
        let reduce = prefs.reduce;
        let base_style = prefs.glass(self.style);
        glass::publish(&ctx, base_style);

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            let avail = ui.max_rect();
            let grid = Grid::new(avail, prefs.gap);
            self.cols = grid.cols;
            self.rows = grid.rows;
            self.relocate();

            let full = avail.shrink(MARGIN);
            let (time, dt, focused) = ctx.input(|i| (i.time, i.stable_dt.min(0.05), i.viewport().focused.unwrap_or(true)));
            let ctrl = ctx.input(|i| mod_held(&i.modifiers, prefs.scroll_mod));
            let pointer = ctx.pointer_hover_pos();
            let fps = if !focused {
                8
            } else if reduce {
                24
            } else {
                prefs.fps as u32
            };
            self.warp.strength = prefs.warp_strength;

            self.wallpaper.update(&ctx, time, fps);
            self.warp.prune(time);
            if self.warp.active() || self.editing {
                ctx.request_repaint();
            }

            let mut moving = false;
            let mut rects: Vec<(Rect, Rect)> = Vec::new();
            for t in &self.tiles {
                let base = grid.rect(t.cell);
                let mut direct = false;
                let target = if self.closing.contains(&t.id) {
                    Rect::from_center_size(base.center(), vec2(6.0, 6.0))
                } else if let Some((_, d)) = self.drag.filter(|(id, _)| *id == t.id) {
                    direct = true;
                    base.translate(d)
                } else if let Some((_, d)) = self.resize.filter(|(id, _)| *id == t.id) {
                    direct = true;
                    Rect::from_min_size(base.min, (base.size() + d).max(vec2(CELL * 0.6, CELL * 0.6)))
                } else if self.expanded && self.focus == Some(t.id) {
                    full
                } else {
                    base
                };
                let m = self.motion.entry(t.id).or_insert_with(|| Motion::at(target));
                moving |= m.step(target, dt, direct);
                rects.push((base, m.rect()));
            }
            if moving {
                ctx.request_repaint();
            }

            if !self.expanded {
                if let Some(fid) = self.focus {
                    let settled = self.tiles.iter().zip(&rects).find(|(t, _)| t.id == fid).is_none_or(|(t, (b, r))| {
                        let m = self.motion[&t.id];
                        (r.min - b.min).length() < 1.0 && (r.max - b.max).length() < 1.0 && m.v.iter().all(|v| v.abs() < 2.0)
                    });
                    if settled {
                        self.focus = None;
                    }
                }
            }

            let amount = wheel_amount(&ctx, prefs.scroll_mod);
            if amount != 0.0 && !self.editing {
                if let Some(p) = pointer {
                    let hit = match self.focus.filter(|_| self.expanded) {
                        Some(fid) => self.tiles.iter().position(|t| t.id == fid),
                        None => rects.iter().position(|(_, r)| r.contains(p)),
                    };
                    if let Some(i) = hit {
                        self.tiles[i].scroll(amount, time);
                    }
                }
            }

            let mut slots: Vec<Slot> = Vec::new();
            for (tile, &(_, shown)) in self.tiles.iter().zip(&rects) {
                let mut rect = shown;
                let dragged = self.drag.is_some_and(|(id, _)| id == tile.id);
                let resized = self.resize.is_some_and(|(id, _)| id == tile.id);
                let focused = self.focus == Some(tile.id);

                let mut glass = rect;
                let mut prox = 0.0;
                if self.editing && !dragged && !resized {
                    let phase = tile.id as f32 * 1.7;
                    let t = time as f32;
                    let off = vec2((t * 19.0 + phase).sin() * 1.3, (t * 23.0 + phase * 1.9).cos() * 1.1);
                    rect = rect.translate(off);
                    glass = rect.expand(0.5 * (t * 17.0 + phase).sin());
                } else if dragged || resized {
                    glass = rect.expand(6.0);
                } else if !focused && !reduce && prefs.hover_fx {
                    let target = pointer.map_or(0.0, |p| (1.0 - distance_to(rect, p) / 170.0).clamp(0.0, 1.0));
                    prox = ctx.animate_value_with_time(Id::unique(("prox", tile.id)), target, 0.18);
                    let half = rect.size() / 2.0;
                    let tilt = pointer.map_or(Vec2::ZERO, |p| {
                        let rel = (p - rect.center()) / half;
                        vec2(rel.x.clamp(-1.0, 1.0), rel.y.clamp(-1.0, 1.0))
                    });
                    glass = rect.expand(5.0 * prox).translate(tilt * 2.5 * prox);
                    rect = rect.translate(tilt * 4.0 * prox);
                }

                let mut s = base_style;
                if !self.editing && !reduce {
                    let push = self.warp.field(rect.center(), time);
                    rect = rect.translate(push);
                    glass = glass.translate(push);
                    let shimmer = (push.length() / 18.0).min(1.0);
                    s.refraction += 26.0 * shimmer;
                    s.chromatic = (s.chromatic + 0.4 * shimmer).min(1.0);
                }
                s.brightness *= 1.0 + 0.05 * prox;
                s.specular = (s.specular + 0.1 * prox).min(1.0);

                slots.push(Slot { id: tile.id, rect, glass, style: s, focused, dragged, resized });
            }

            let ghosting = self.focus.is_some();
            let quality = if reduce { LiveBackdropQuality::Performance } else { LiveBackdropQuality::Full };
            {
                let (wallpaper, warp, tiles, live) = (&self.wallpaper, &self.warp, &mut self.tiles, &self.live);
                let radius = base_style.corner_radius;
                live.run_with_quality(ui, Color32::BLACK, quality, |ui| {
                    wallpaper.paint(ui, avail, warp, time);
                    if ghosting {
                        for (tile, slot) in tiles.iter_mut().zip(&slots) {
                            if !slot.focused {
                                tile.ghost(ui, slot.glass, radius);
                            }
                        }
                    }
                });
            }

            let mut outs: Vec<(u64, Out)> = Vec::new();
            for (tile, slot) in self.tiles.iter_mut().zip(&slots) {
                if (ghosting && !slot.focused) || self.closing.contains(&slot.id) {
                    outs.push((slot.id, Out::default()));
                    continue;
                }
                let view = View {
                    expanded: self.expanded && slot.focused,
                    editing: self.editing,
                    ctrl,
                };
                let order = if slot.focused || slot.dragged || slot.resized { Order::Foreground } else { Order::Middle };

                let mut out = Out::default();
                egui::Area::new(Id::unique(("tile", slot.id)))
                    .order(order)
                    .fixed_pos(slot.rect.min)
                    .movable(false)
                    .constrain(false)
                    .show(&ctx, |ui| {
                        let (r, _) = ui.allocate_exact_size(slot.rect.size(), Sense::hover());
                        out = tile.show(ui, r, slot.glass, &slot.style, &view);
                    });
                outs.push((slot.id, out));
            }

            let pet_click = ctx.input(|i| if i.pointer.primary_pressed() { i.pointer.interact_pos() } else { None });
            let on_pet = prefs.pet && pet_click.is_some_and(|c| self.pet.hit(c, prefs.pet_scale));
            let press = ctx.input(|i| {
                let p = i.pointer.hover_pos()?;
                if i.pointer.button_pressed(PointerButton::Primary) {
                    Some((p, false))
                } else if i.pointer.button_pressed(PointerButton::Secondary) {
                    Some((p, true))
                } else {
                    None
                }
            });
            let on_tile = outs.iter().any(|(_, o)| o.body_hovered);
            let blank = |p: Pos2| !on_tile && ctx.layer_id_at(p).is_none_or(|l| l.order == Order::Background);
            if let (Some((p, right)), false, false) = (press, self.editing, reduce || !prefs.warp || on_pet) {
                if on_tile || blank(p) {
                    self.warp.push(p, time, right);
                }
            }

            if !ctx.input(|i| i.pointer.primary_down()) {
                self.hold = None;
            } else if let Some((p, false)) = press {
                self.hold = blank(p).then_some((p, time));
            }
            if let Some((origin, since)) = self.hold {
                if self.editing || pointer.is_none_or(|q| q.distance(origin) > 10.0) {
                    self.hold = None;
                } else if time - since > 0.55 {
                    self.editing = true;
                    self.expanded = false;
                    self.hold = None;
                } else {
                    ctx.request_repaint();
                }
            }
            self.warp.draw_rings(&ctx, time);

            for (id, out) in outs {
                if out.expand {
                    if self.expanded && self.focus == Some(id) {
                        self.expanded = false;
                    } else {
                        self.focus = Some(id);
                        self.expanded = true;
                    }
                }
                if out.long_press && !self.editing {
                    self.editing = true;
                    self.expanded = false;
                }
                if out.remove {
                    self.closing.insert(id);
                    continue;
                }
                if let Some(d) = out.drag {
                    let acc = self.drag.filter(|(i, _)| *i == id).map_or(Vec2::ZERO, |(_, a)| a);
                    self.drag = Some((id, acc + d));
                }
                if out.drag_end {
                    self.finish_drag(&grid, id);
                }
                if let Some(d) = out.resize {
                    let acc = self.resize.filter(|(i, _)| *i == id).map_or(Vec2::ZERO, |(_, a)| a);
                    self.resize = Some((id, acc + d));
                }
                if out.resize_end {
                    self.finish_resize(&grid, id);
                }
            }

            let gone: Vec<u64> = self
                .closing
                .iter()
                .copied()
                .filter(|id| self.motion.get(id).is_none_or(|m| m.rect().width() < 14.0))
                .collect();
            for id in gone {
                self.closing.remove(&id);
                self.motion.remove(&id);
                self.tiles.retain(|t| t.id != id);
                if self.focus == Some(id) {
                    self.focus = None;
                    self.expanded = false;
                }
            }

            if prefs.pet {
                let plats = self.platforms(&slots, avail);
                let active = ctx.input(|i| i.pointer.is_moving() || i.pointer.any_down() || !i.events.is_empty());
                let input = PetInput {
                    cursor: pointer,
                    click: pet_click,
                    active,
                    follow: prefs.pet_follow && !self.editing,
                    interval: prefs.pet_interval,
                    scale: prefs.pet_scale,
                };
                self.pet.update(dt, &plats, avail, &input);
                let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("pet")));
                self.pet.paint(&painter, &self.sheet, prefs.pet_scale);
                ctx.request_repaint_after(self.pet.repaint_after(reduce));
            }

            if self.editing {
                let mut add = None;
                let mut done = false;
                let bar_style = glass::popup_style(base_style).with_corner_radius(f32::INFINITY);
                egui::Area::new(Id::unique("edit_bar"))
                    .order(Order::Foreground)
                    .anchor(Align2::CENTER_TOP, vec2(0.0, 14.0))
                    .show(&ctx, |ui| {
                        Glass::new(bar_style)
                            .inner_margin(egui::Margin::symmetric(14, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let plus = ui.button("+  Add");
                                    glass::menu(&plus, |ui| {
                                        for k in Kind::ALL {
                                            if ui.button(k.name()).clicked() {
                                                add = Some(k);
                                                ui.close();
                                            }
                                        }
                                    });
                                    if ui.button("Done").clicked() {
                                        done = true;
                                    }
                                });
                            });
                    });
                if let Some(k) = add {
                    self.add_tile(k, &grid);
                }
                if done {
                    self.end_edit();
                }
            }

            if time - self.last_save >= 2.0 {
                self.last_save = time;
                self.save_now();
            }
            ctx.request_repaint_after(Duration::from_secs(2));
        });
    }

    fn on_exit(&mut self) {
        self.save_now();
    }
}

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Tilezone")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Tilezone", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
