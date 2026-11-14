use eframe::egui;
use egui_glass::{Glass, GlassStyle};

struct App {
    style: GlassStyle,
    tint_alpha: u8,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe must use the wgpu renderer");

        egui_glass::init(rs, 1);

        // Not one colour as glass refraction will look bare.
        egui_glass::set_backdrop(&cc.egui_ctx, rs, &dyn_wallpaper());

        let style = GlassStyle::clear();
        Self {
            tint_alpha: style.tint.a(),
            style,
        }
    }

    fn load_preset(&mut self, style: GlassStyle) {
        self.tint_alpha = style.tint.a();
        self.style = style;
    }

    fn as_rust(&self) -> String {
        let s = &self.style;
        format!(
            "GlassStyle {{ corner_radius: {:?}, corner_smoothing: {:?}, blur: {:?}, refraction: {:?}, \
             edge_width: {:?}, chromatic: {:?}, tint: egui::Color32::from_rgba_unmultiplied(255, 255, 255, {}), \
             brightness: {:?}, saturation: {:?}, specular: {:?}, border: {:?}, shadow: {:?}, \
             shadow_radius: {:?}, shadow_offset: {:?}, shadow_spread: {:?} }}",
            s.corner_radius,
            s.corner_smoothing,
            s.blur,
            s.refraction,
            s.edge_width,
            s.chromatic,
            self.tint_alpha,
            s.brightness,
            s.saturation,
            s.specular,
            s.border,
            s.shadow,
            s.shadow_radius,
            s.shadow_offset,
            s.shadow_spread,
        )
    }
}

fn dyn_wallpaper() -> egui::ColorImage {
    const W: usize = 1024;
    const H: usize = 1024;
    let mut img = egui::ColorImage::filled([W, H], egui::Color32::BLACK);

    let blobs = [
        (0.22, 0.28, 0.30, 255.0, 80.0, 220.0),
        (0.78, 0.35, 0.26, 120.0, 90.0, 255.0),
        (0.50, 0.80, 0.34, 218.0, 112.0, 214.0),
        (0.88, 0.85, 0.20, 190.0, 20.0, 255.0),
    ];

    for y in 0..H {
        for x in 0..W {
            let u = x as f32 / W as f32;
            let v = y as f32 / H as f32;

            // deep violet > dark indigo
            let t = (u + v) * 0.5;
            let mut r = 25.0 + 45.0 * t;
            let mut g = 8.0 + 14.0 * t;
            let mut b = 55.0 + 85.0 * (1.0 - t);

            for (bx, by, rad, br, bg, bb) in blobs {
                let d = ((u - bx) * (u - bx) + (v - by) * (v - by)).sqrt();
                let k = (1.0 - d / rad).clamp(0.0, 1.0);
                let k = k * k * (3.0 - 2.0 * k);
                r += br * k;
                g += bg * k;
                b += bb * k;
            }

            if x % 64 < 2 || y % 64 < 2 {
                r += 70.0;
                g += 40.0;
                b += 110.0;
            }
            
            let dx = x as f32 - W as f32 * 0.5;
            let dy = y as f32 - H as f32 * 0.5;
            if (dx * dx + dy * dy).sqrt() % 48.0 < 3.0 {
                r += 120.0;
                g += 60.0;
                b += 160.0;
            }
            
            if (x + y) % 192 < 14 {
                r += 90.0;
                g += 0.0;
                b += 150.0;
            }

            img[(x, y)] = egui::Color32::from_rgb(
                r.min(255.0) as u8,
                g.min(255.0) as u8,
                b.min(255.0) as u8,
            );
        }
    }
    img
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui_glass::show_backdrop(ui, ui.max_rect());

                // Tile
                egui::Area::new(egui::Id::unique("tile_cpu"))
                    .default_pos(egui::pos2(60.0, 60.0))
                    .movable(true)
                    .constrain(false)
                    .show(ui.ctx(), |ui| {
                        Glass::new(self.style).show(ui, |ui| {
                            ui.set_min_size(egui::vec2(220.0, 120.0));
                            ui.heading("CPU");
                            ui.label("drag me over the grid lines");
                        });
                    });

                // Tile 2
                egui::Area::new(egui::Id::unique("tile_todo"))
                    .default_pos(egui::pos2(360.0, 120.0))
                    .movable(true)
                    .constrain(false)
                    .show(ui.ctx(), |ui| {
                        Glass::new(self.style).show(ui, |ui| {
                            ui.set_min_size(egui::vec2(260.0, 200.0));
                            ui.heading("Todo");
                            ui.label("nqetm's liquid glass spike");
                        });
                    });

                // Tile 3
                egui::Area::new(egui::Id::unique("tile_pill"))
                    .default_pos(egui::pos2(120.0, 320.0))
                    .movable(true)
                    .constrain(false)
                    .show(ui.ctx(), |ui| {
                        Glass::new(self.style.with_corner_radius(f32::INFINITY)).show(ui, |ui| {
                            ui.set_min_size(egui::vec2(180.0, 40.0));
                            ui.label("capsule");
                        });
                    });
            });

        egui::Window::new("glass tuner")
            .default_pos(egui::pos2(700.0, 40.0))
            .show(ui.ctx(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("regular").clicked() {
                        self.load_preset(GlassStyle::regular());
                    }
                    if ui.button("clear").clicked() {
                        self.load_preset(GlassStyle::clear());
                    }
                    if ui.button("panel").clicked() {
                        self.load_preset(GlassStyle::panel());
                    }
                    if ui.button("dark").clicked() {
                        self.load_preset(GlassStyle::dark());
                    }
                });
                ui.separator();

                ui.add(egui::Slider::new(&mut self.style.corner_radius, 0.0..=60.0).text("corner radius"));
                ui.add(egui::Slider::new(&mut self.style.corner_smoothing, 0.0..=1.0).text("corner smoothing"));
                ui.add(egui::Slider::new(&mut self.style.blur, 0.0..=40.0).text("blur"));
                ui.add(egui::Slider::new(&mut self.style.refraction, 0.0..=60.0).text("refraction"));
                ui.add(egui::Slider::new(&mut self.style.edge_width, 1.0..=80.0).text("edge width"));
                ui.add(egui::Slider::new(&mut self.style.chromatic, 0.0..=1.0).text("chromatic"));
                ui.add(egui::Slider::new(&mut self.tint_alpha, 0..=255).text("tint alpha"));
                ui.add(egui::Slider::new(&mut self.style.brightness, 0.5..=1.5).text("brightness"));
                ui.add(egui::Slider::new(&mut self.style.saturation, 0.0..=2.0).text("saturation"));
                ui.add(egui::Slider::new(&mut self.style.specular, 0.0..=1.0).text("specular"));
                ui.add(egui::Slider::new(&mut self.style.border, 0.0..=1.0).text("border"));
                ui.add(egui::Slider::new(&mut self.style.shadow, 0.0..=1.0).text("shadow"));

                self.style.tint =
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, self.tint_alpha);

                ui.separator();
                if ui.button("copy these values").clicked() {
                    let code = self.as_rust();
                    ui.ctx().copy_text(code);
                }
            });
    }
}

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "glass spike",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}