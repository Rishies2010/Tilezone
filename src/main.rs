use eframe::egui;
use egui_glass::{Glass, GlassStyle};

const MAX_SIDE: u32 = 3840;

struct App {
    rs: eframe::egui_wgpu::RenderState,
    style: GlassStyle,
    tint_alpha: u8,
    image_path: String,
    image_err: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .clone()
            .expect("eframe must use the wgpu renderer");

        egui_glass::init(&rs, 1);
        egui_glass::set_backdrop(&cc.egui_ctx, &rs, &fallback_wallpaper());

        let style = GlassStyle {
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
        };
        let mut app = Self {
            rs,
            tint_alpha: style.tint.a(),
            style,
            image_path: std::env::args().nth(1).unwrap_or_else(|| "wallpaper.jpg".into()),
            image_err: None,
        };
        app.load_backdrop(&cc.egui_ctx);
        app
    }

    fn load_backdrop(&mut self, ctx: &egui::Context) {
        match load_image(self.image_path.trim().trim_matches('"')) {
            Ok(img) => {
                egui_glass::set_backdrop(ctx, &self.rs, &img);
                self.image_err = None;
            }
            Err(e) => self.image_err = Some(e),
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

fn load_image(path: &str) -> Result<egui::ColorImage, String> {
    let mut img = image::open(path).map_err(|e| e.to_string())?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.thumbnail(MAX_SIDE, MAX_SIDE);
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

// fallback
fn fallback_wallpaper() -> egui::ColorImage {
    const N: usize = 1024;
    let mut img = egui::ColorImage::filled([N, N], egui::Color32::BLACK);
    for y in 0..N {
        for x in 0..N {
            let mut v = 20 + (x + y) / 16;
            if x % 64 < 2 || y % 64 < 2 {
                v += 40;
            }
            let v = v as u8;
            img[(x, y)] = egui::Color32::from_rgb(v, v, v + 15);
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
                ui.horizontal(|ui| {
                    let field = ui.text_edit_singleline(&mut self.image_path);
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("load").clicked() || enter {
                        self.load_backdrop(ui.ctx());
                    }
                });
                if let Some(err) = &self.image_err {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                }
                ui.separator();

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
        "Tilezone",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}