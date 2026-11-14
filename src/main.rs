use eframe::egui;
use egui_glass::Glass;

struct App;

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("eframe must use the wgpu renderer");

        egui_glass::init(rs, 1);

        // Not one colour as glass refraction will look bare.
        egui_glass::set_backdrop(&cc.egui_ctx, rs, &dyn_wallpaper());
        Self
    }
}

/// Procedural orchid / neon-purple wallpaper with a few bright blobs,
/// so refraction and blur are actually visible.
fn dyn_wallpaper() -> egui::ColorImage {
    const W: usize = 512;
    const H: usize = 512;
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

                // Tile.
                egui::Area::new(egui::Id::unique("tile_cpu"))
                    .default_pos(egui::pos2(80.0, 80.0))
                    .movable(true)
                    .constrain(false)
                    .show(ui.ctx(), |ui| {
                        Glass::default().show(ui, |ui| {
                            ui.set_min_size(egui::vec2(220.0, 120.0));
                            ui.heading("CPU");
                            ui.label("999% usage");
                        });
                    });

                // Tile 2, bigger
                egui::Area::new(egui::Id::unique("tile_todo"))
                    .default_pos(egui::pos2(380.0, 160.0))
                    .movable(true)
                    .constrain(false)
                    .show(ui.ctx(), |ui| {
                        Glass::panel().show(ui, |ui| {
                            ui.set_min_size(egui::vec2(260.0, 200.0));
                            ui.heading("Todo");
                            ui.label("Finish this app");
                        });
                    });
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