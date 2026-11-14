mod applets;
mod tile;

use eframe::egui::{self, vec2, Color32, Id, Order, Rect, Sense};
use egui_glass::GlassStyle;

use applets::{ClickSpeed, Notepad, SystemStats, Todo};
use tile::Tile;

const MAX_SIDE: u32 = 3840;
const COLS: f32 = 6.0;
const ROWS: f32 = 4.0;
const GAP: f32 = 16.0;
const MARGIN: f32 = 28.0;

struct App {
    rs: eframe::egui_wgpu::RenderState,
    style: GlassStyle,
    tiles: Vec<Tile>,
    focus: Option<usize>,
    expanded: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must use the wgpu renderer");

        egui_glass::init(&rs, 1);
        egui_glass::set_backdrop(&cc.egui_ctx, &rs, &fallback_wallpaper());
        cc.egui_ctx.set_visuals(visuals());

        let app = Self {
            rs,
            style: glass_style(),
            tiles: vec![
                Tile::new("system", (0, 0, 2, 2), vec![Box::new(SystemStats::new())]),
                Tile::new("notes", (2, 0, 2, 3), vec![Box::new(Todo::new()), Box::new(Notepad::new())]),
                Tile::new("games", (4, 0, 2, 2), vec![Box::new(ClickSpeed::new())]),
            ],
            focus: None,
            expanded: false,
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
}

fn glass_style() -> GlassStyle {
    GlassStyle {
        corner_radius: 40.0,
        corner_smoothing: 1.0,
        blur: 2.0,
        refraction: 60.0,
        edge_width: 50.0,
        chromatic: 0.3,
        tint: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 0),
        brightness: 1.0,
        saturation: 1.2,
        specular: 0.9,
        border: 0.5,
        shadow: 0.0,
        ..GlassStyle::clear()
    }
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(Color32::from_white_alpha(235));
    v.extreme_bg_color = Color32::from_white_alpha(18);
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

fn grid_rect(avail: Rect, (col, row, w, h): (usize, usize, usize, usize)) -> Rect {
    let cw = (avail.width() - 2.0 * MARGIN - (COLS - 1.0) * GAP) / COLS;
    let ch = (avail.height() - 2.0 * MARGIN - (ROWS - 1.0) * GAP) / ROWS;
    let min = avail.min + vec2(MARGIN + col as f32 * (cw + GAP), MARGIN + row as f32 * (ch + GAP));
    let size = vec2(w as f32 * cw + (w as f32 - 1.0) * GAP, h as f32 * ch + (h as f32 - 1.0) * GAP);
    Rect::from_min_size(min, size)
}

fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::from_min_max(a.min.lerp(b.min, t), a.max.lerp(b.max, t))
}

fn wheel_amount(ctx: &egui::Context) -> f32 {
    ctx.input(|i| {
        i.events
            .iter()
            .map(|e| match e {
                egui::Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.ctrl && modifiers.shift => {
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
            self.set_wallpaper(&ctx, &path.to_string_lossy());
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.expanded = false;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui_glass::show_backdrop(ui, ui.max_rect());
                let avail = ui.max_rect();

                let t = ctx.animate_bool_with_time(Id::unique("expand"), self.expanded, 0.3);
                if t == 0.0 && !self.expanded {
                    self.focus = None;
                }
                let eased = t * t * (3.0 - 2.0 * t);
                let full = avail.shrink(MARGIN);

                let rects: Vec<Rect> = self
                    .tiles
                    .iter()
                    .enumerate()
                    .map(|(i, tile)| {
                        let base = grid_rect(avail, tile.cell);
                        if self.focus == Some(i) {
                            lerp_rect(base, full, eased)
                        } else {
                            base
                        }
                    })
                    .collect();

                let amount = wheel_amount(&ctx);
                if amount != 0.0 {
                    if let Some(pos) = ctx.pointer_hover_pos() {
                        let hit = match self.focus {
                            Some(i) => rects[i].contains(pos).then_some(i),
                            None => rects.iter().position(|r| r.contains(pos)),
                        };
                        if let Some(i) = hit {
                            let now = ctx.input(|i| i.time);
                            self.tiles[i].scroll(amount, now);
                        }
                    }
                }

                let (style, expanded, focus) = (self.style, self.expanded, self.focus);
                let mut toggle = None;
                for (i, tile) in self.tiles.iter_mut().enumerate() {
                    let order = if focus == Some(i) { Order::Foreground } else { Order::Middle };
                    egui::Area::new(Id::unique(("tile", tile.id)))
                        .order(order)
                        .fixed_pos(rects[i].min)
                        .movable(false)
                        .show(&ctx, |ui| {
                            let (rect, _) = ui.allocate_exact_size(rects[i].size(), Sense::hover());
                            let is_open = expanded && focus == Some(i);
                            if tile.show(ui, rect, &style, is_open) {
                                toggle = Some(i);
                            }
                        });
                }

                if let Some(i) = toggle {
                    if self.expanded && self.focus == Some(i) {
                        self.expanded = false;
                    } else {
                        self.focus = Some(i);
                        self.expanded = true;
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
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "glass spike",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}