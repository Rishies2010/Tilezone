mod applets;
mod tile;

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontFamily, FontId, Id, LayerId, Order, PointerButton, Pos2, Rect, Sense,
    Stroke, TextStyle, Vec2,
};
use egui_glass::{Glass, GlassStyle};

use applets::{ClickSpeed, Clipboard, Kind, Notepad, Photo, Snake, SystemStats, Todo};
use tile::{Cell, Out, Tile, View};

const MAX_SIDE: u32 = 3840;
const CELL: f32 = 112.0;
const GAP: f32 = 12.0;
const MARGIN: f32 = 24.0;

struct Grid {
    origin: Pos2,
    cols: usize,
    rows: usize,
}

impl Grid {
    fn new(avail: Rect) -> Self {
        let fit = |len: f32| (((len - 2.0 * MARGIN + GAP) / (CELL + GAP)).floor() as usize).max(1);
        let (cols, rows) = (fit(avail.width()), fit(avail.height()));
        let size = vec2(cols as f32, rows as f32) * (CELL + GAP) - vec2(GAP, GAP);
        Self {
            origin: avail.center() - size / 2.0,
            cols,
            rows,
        }
    }

    fn rect(&self, (c, r, w, h): Cell) -> Rect {
        let min = self.origin + vec2(c as f32, r as f32) * (CELL + GAP);
        let size = vec2(w as f32, h as f32) * CELL + vec2(w as f32 - 1.0, h as f32 - 1.0) * GAP;
        Rect::from_min_size(min, size)
    }
}

fn overlaps(a: Cell, b: Cell) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

struct Ripple {
    pos: Pos2,
    t0: f64,
    right: bool,
}

struct App {
    rs: eframe::egui_wgpu::RenderState,
    style: GlassStyle,
    tiles: Vec<Tile>,
    next_id: u64,
    cols: usize,
    rows: usize,
    last_size: Vec2,
    focus: Option<u64>,
    expanded: bool,
    editing: bool,
    drag: Option<(u64, Vec2)>,
    resize: Option<(u64, Vec2)>,
    ripples: Vec<Ripple>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must use the wgpu renderer");

        egui_glass::init(&rs, 1);
        egui_glass::set_backdrop(&cc.egui_ctx, &rs, &fallback_wallpaper());
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);

        let app = Self {
            rs,
            style: glass_style(),
            tiles: vec![
                Tile::new(1, (0, 0, 4, 1), vec![Box::new(SystemStats::new())]),
                Tile::new(2, (0, 1, 2, 2), vec![Box::new(Todo::new()), Box::new(Notepad::new())]),
                Tile::new(3, (2, 1, 2, 2), vec![Box::new(Clipboard::new())]),
                Tile::new(4, (4, 0, 2, 2), vec![Box::new(Photo::new())]),
                Tile::new(5, (4, 2, 2, 2), vec![Box::new(ClickSpeed::new()), Box::new(Snake::new())]),
            ],
            next_id: 5,
            cols: 6,
            rows: 4,
            last_size: Vec2::ZERO,
            focus: None,
            expanded: false,
            editing: false,
            drag: None,
            resize: None,
            ripples: Vec::new(),
        };

        let path = std::env::args().nth(1).unwrap_or_else(|| "wallpaper.jpg".into());
        app.set_wallpaper(&cc.egui_ctx, &path);
        app
    }

    fn set_wallpaper(&self, ctx: &egui::Context, path: &str) {
        match load_image(path.trim().trim_matches('"')) {
            Ok(img) => {
                egui_glass::set_backdrop(ctx, &self.rs, &img);
            }
            Err(e) => eprintln!("wallpaper: {e}"),
        }
    }

    fn is_free(&self, skip: Option<u64>, cell: Cell) -> bool {
        cell.0 + cell.2 <= self.cols
            && cell.1 + cell.3 <= self.rows
            && self
                .tiles
                .iter()
                .filter(|t| Some(t.id) != skip)
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

    fn add_tile(&mut self, kind: Kind) {
        let span = kind.span();
        if let Some(cell) = self.find_spot(None, &[span, (2, 1), (1, 1)]) {
            self.next_id += 1;
            self.tiles.push(Tile::new(self.next_id, cell, vec![kind.build()]));
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
        let step = CELL + GAP;
        let col = (((min.x - grid.origin.x) / step).round() as i32).clamp(0, self.cols.saturating_sub(cell.2) as i32);
        let row = (((min.y - grid.origin.y) / step).round() as i32).clamp(0, self.rows.saturating_sub(cell.3) as i32);
        self.set_cell(id, (col as usize, row as usize, cell.2, cell.3));
    }

    fn finish_resize(&mut self, grid: &Grid, id: u64) {
        let Some((_, d)) = self.resize.take() else { return };
        let Some(cell) = self.cell_of(id) else { return };
        let size = grid.rect(cell).size() + d;
        let step = CELL + GAP;
        let w = (((size.x + GAP) / step).round() as i32).clamp(1, self.cols.saturating_sub(cell.0).max(1) as i32);
        let h = (((size.y + GAP) / step).round() as i32).clamp(1, self.rows.saturating_sub(cell.1).max(1) as i32);
        self.set_cell(id, (cell.0, cell.1, w as usize, h as usize));
    }

    fn end_edit(&mut self) {
        self.editing = false;
        self.drag = None;
        self.resize = None;
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
        ..GlassStyle::clear()
    }
}

fn setup_fonts(ctx: &egui::Context) {
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

    ctx.set_fonts(fonts);
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

fn load_image(path: &str) -> Result<egui::ColorImage, String> {
    let mut img = image::open(path).map_err(|e| e.to_string())?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.thumbnail(MAX_SIDE, MAX_SIDE);
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

// only shown until a real image loads
fn fallback_wallpaper() -> egui::ColorImage {
    const N: usize = 1024;
    let mut img = egui::ColorImage::filled([N, N], Color32::BLACK);
    for y in 0..N {
        for x in 0..N {
            let mut v = 20 + (x + y) / 16;
            if x % 64 < 2 || y % 64 < 2 {
                v += 40;
            }
            let v = v as u8;
            img[(x, y)] = Color32::from_rgb(v, v, v + 15);
        }
    }
    img
}

fn animate_rect(ctx: &egui::Context, id: u64, r: Rect, secs: f32) -> Rect {
    let a = |k: &str, v: f32| ctx.animate_value_with_time(Id::unique(("rect", id, k.to_string())), v, secs);
    Rect::from_min_max(pos2(a("x0", r.min.x), a("y0", r.min.y)), pos2(a("x1", r.max.x), a("y1", r.max.y)))
}

fn wheel_amount(ctx: &egui::Context) -> f32 {
    ctx.input(|i| {
        i.events
            .iter()
            .map(|e| match e {
                egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.ctrl => {
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

// damped spring: tiles get pushed away from a left click and pulled toward a right click
fn impulse(ripples: &[(Pos2, f32, bool)], center: Pos2) -> Vec2 {
    let mut v = Vec2::ZERO;
    for &(pos, age, right) in ripples {
        let d = center - pos;
        let dist = d.length().max(1.0);
        let amp = 7.0 * (-dist / 240.0).exp() * (age * 15.0).sin() * (-age * 4.5).exp();
        v += d / dist * amp * if right { -1.0 } else { 1.0 };
    }
    v
}

const LEFT_RINGS: [(f32, f32, f32); 1] = [(0.0, 150.0, 1.0)];
const RIGHT_RINGS: [(f32, f32, f32); 2] = [(0.0, 120.0, 1.0), (0.14, 90.0, 0.7)];

fn draw_ripples(ctx: &egui::Context, ripples: &[Ripple], time: f64) {
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("ripples")));
    for r in ripples {
        let age = (time - r.t0) as f32;
        let rings: &[(f32, f32, f32)] = if r.right { &RIGHT_RINGS } else { &LEFT_RINGS };
        for &(delay, max_r, strength) in rings {
            let t = (age - delay) / 0.75;
            if !(0.0..1.0).contains(&t) {
                continue;
            }
            let eased = 1.0 - (1.0 - t).powi(3);
            let fade = (1.0 - t).powf(1.6) * strength;
            let radius = 6.0 + max_r * eased;
            painter.circle_filled(r.pos, radius, Color32::from_white_alpha((14.0 * fade) as u8));
            painter.circle_stroke(
                r.pos,
                radius,
                Stroke::new(1.0 + 2.0 * (1.0 - t), Color32::from_white_alpha((170.0 * fade) as u8)),
            );
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.editing {
                self.end_edit();
            } else {
                self.expanded = false;
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui_glass::show_backdrop(ui, ui.max_rect());
                let avail = ui.max_rect();
                let grid = Grid::new(avail);
                self.cols = grid.cols;
                self.rows = grid.rows;
                self.relocate();

                let resizing = (avail.size() - self.last_size).length() > 0.5;
                self.last_size = avail.size();
                let full = avail.shrink(MARGIN);
                let time = ctx.input(|i| i.time);
                let ctrl = ctx.input(|i| i.modifiers.ctrl);
                let pointer = ctx.pointer_hover_pos();
                if self.editing {
                    ctx.request_repaint();
                }

                self.ripples.retain(|r| time - r.t0 < 1.0);
                if !self.ripples.is_empty() {
                    ctx.request_repaint();
                }
                let waves: Vec<(Pos2, f32, bool)> = self
                    .ripples
                    .iter()
                    .map(|r| (r.pos, (time - r.t0) as f32, r.right))
                    .collect();

                let mut rects: Vec<(Rect, Rect)> = Vec::new();
                for t in &self.tiles {
                    let base = grid.rect(t.cell);
                    let calm = if resizing { 0.0 } else { 0.3 };
                    let (target, secs) = if let Some((_, d)) = self.drag.filter(|(id, _)| *id == t.id) {
                        (base.translate(d), 0.0)
                    } else if let Some((_, d)) = self.resize.filter(|(id, _)| *id == t.id) {
                        let size = (base.size() + d).max(vec2(CELL * 0.6, CELL * 0.6));
                        (Rect::from_min_size(base.min, size), 0.0)
                    } else if self.expanded && self.focus == Some(t.id) {
                        (full, calm)
                    } else {
                        (base, calm)
                    };
                    rects.push((base, animate_rect(&ctx, t.id, target, secs)));
                }

                if !self.expanded {
                    if let Some(fid) = self.focus {
                        let settled = self.tiles.iter().zip(&rects).find(|(t, _)| t.id == fid).is_none_or(
                            |(_, (b, r))| (r.min - b.min).length() < 1.0 && (r.max - b.max).length() < 1.0,
                        );
                        if settled {
                            self.focus = None;
                        }
                    }
                }

                if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
                    let at = ctx.input(|i| i.pointer.latest_pos());
                    let mut taken = false;
                    if let Some(i) = at.and_then(|p| rects.iter().position(|(_, r)| r.contains(p))) {
                        taken = self.tiles[i].on_drop(&ctx, &path);
                    }
                    if !taken {
                        self.set_wallpaper(&ctx, &path.to_string_lossy());
                    }
                }

                let amount = wheel_amount(&ctx);
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

                let style = self.style;
                let mut outs: Vec<(u64, Out)> = Vec::new();
                for (i, tile) in self.tiles.iter_mut().enumerate() {
                    let (_, mut rect) = rects[i];
                    let dragged = self.drag.is_some_and(|(id, _)| id == tile.id);
                    let resized = self.resize.is_some_and(|(id, _)| id == tile.id);
                    let focused = self.focus == Some(tile.id);
                    let is_open = self.expanded && focused;

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
                    } else if !focused {
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

                    if !self.editing {
                        let push = impulse(&waves, rect.center());
                        rect = rect.translate(push);
                        glass = glass.translate(push);
                    }

                    let mut s = style;
                    s.brightness *= 1.0 + 0.05 * prox;
                    s.specular = (s.specular + 0.1 * prox).min(1.0);

                    let view = View {
                        expanded: is_open,
                        editing: self.editing,
                        ctrl,
                    };
                    let order = if focused || dragged || resized { Order::Foreground } else { Order::Middle };

                    let mut out = Out::default();
                    egui::Area::new(Id::unique(("tile", tile.id)))
                        .order(order)
                        .fixed_pos(rect.min)
                        .movable(false)
                        .constrain(false)
                        .show(&ctx, |ui| {
                            let (r, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                            out = tile.show(ui, r, glass, &s, &view);
                        });
                    outs.push((tile.id, out));
                }

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
                if let (Some((p, right)), false) = (press, self.editing) {
                    let on_tile = outs.iter().any(|(_, o)| o.body_hovered);
                    let on_backdrop = ctx.layer_id_at(p).is_none_or(|l| l.order == Order::Background);
                    if on_tile || on_backdrop {
                        self.ripples.push(Ripple { pos: p, t0: time, right });
                    }
                }
                draw_ripples(&ctx, &self.ripples, time);

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
                        self.tiles.retain(|t| t.id != id);
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

                if self.editing {
                    let mut add = None;
                    let mut done = false;
                    egui::Area::new(Id::unique("edit_bar"))
                        .order(Order::Foreground)
                        .anchor(Align2::CENTER_TOP, vec2(0.0, 14.0))
                        .show(&ctx, |ui| {
                            Glass::new(style.with_corner_radius(f32::INFINITY))
                                .inner_margin(egui::Margin::symmetric(14, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let plus = ui.button("+  Add");
                                        egui::Popup::menu(&plus).show(|ui| {
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
                        self.add_tile(k);
                    }
                    if done {
                        self.end_edit();
                    }
                }
            });
    }
}

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([800.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "glass spike",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}