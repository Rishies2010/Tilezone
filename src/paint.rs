use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, pos2, vec2, Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui};

use serde_json::{json, Value};

use crate::applets::{dim, ghost_fill, Applet, Kind};

const CW: usize = 960;
const CH: usize = 640;
const SWATCHES: [[u8; 3]; 8] = [
    [255, 255, 255],
    [24, 22, 34],
    [192, 96, 255],
    [218, 112, 214],
    [255, 64, 160],
    [64, 200, 255],
    [120, 255, 160],
    [255, 214, 64],
];
const SIZES: [f32; 3] = [3.0, 9.0, 22.0];
const UNDO_DEPTH: usize = 8;

pub struct Paint {
    px: Vec<u8>,
    tex: Option<TextureHandle>,
    dirty: Option<[usize; 4]>,
    full: bool,
    color: usize,
    size: usize,
    erase: bool,
    last: Option<Pos2>,
    undo: Vec<Vec<u8>>,
    saving: Option<Receiver<bool>>,
    saved_at: f64,
    uid: u64,
    unsaved: bool,
}

impl Paint {
    pub fn new() -> Self {
        Self {
            px: vec![0; CW * CH * 4],
            tex: None,
            dirty: None,
            full: true,
            color: 2,
            size: 1,
            erase: false,
            last: None,
            undo: Vec::new(),
            saving: None,
            saved_at: f64::NEG_INFINITY,
            uid: SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64),
            unsaved: false,
        }
    }

    fn mark(&mut self, x0: usize, y0: usize, x1: usize, y1: usize) {
        self.unsaved = true;
        self.dirty = Some(match self.dirty {
            Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1), d.max(y1)],
            None => [x0, y0, x1, y1],
        });
    }

    fn stamp(&mut self, c: Pos2, r: f32) {
        let x0 = (c.x - r - 1.0).floor().max(0.0) as usize;
        let y0 = (c.y - r - 1.0).floor().max(0.0) as usize;
        let x1 = ((c.x + r + 1.0).ceil().max(0.0) as usize).min(CW);
        let y1 = ((c.y + r + 1.0).ceil().max(0.0) as usize).min(CH);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let rgb = SWATCHES[self.color];
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - c.x).powi(2) + (y as f32 + 0.5 - c.y).powi(2)).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                let i = (y * CW + x) * 4;
                let dst_a = self.px[i + 3] as f32 / 255.0;
                if self.erase {
                    self.px[i + 3] = (dst_a * (1.0 - cov) * 255.0).round() as u8;
                } else {
                    let out_a = cov + dst_a * (1.0 - cov);
                    for k in 0..3 {
                        let dst = self.px[i + k] as f32;
                        let v = (rgb[k] as f32 * cov + dst * dst_a * (1.0 - cov)) / out_a;
                        self.px[i + k] = v.round() as u8;
                    }
                    self.px[i + 3] = (out_a * 255.0).round() as u8;
                }
            }
        }
        self.mark(x0, y0, x1, y1);
    }

    fn line(&mut self, a: Pos2, b: Pos2, r: f32) {
        let step = (r * 0.3).max(0.6);
        let n = ((b - a).length() / step).ceil().max(1.0) as usize;
        for k in 0..=n {
            self.stamp(a.lerp(b, k as f32 / n as f32), r);
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(self.px.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    fn flush(&mut self, ctx: &egui::Context) {
        let opts = TextureOptions::LINEAR;
        let Some(tex) = &mut self.tex else {
            let image = ColorImage::from_rgba_unmultiplied([CW, CH], &self.px);
            self.tex = Some(ctx.load_texture("paint_canvas", image, opts));
            self.full = false;
            self.dirty = None;
            return;
        };
        if self.full {
            tex.set(ColorImage::from_rgba_unmultiplied([CW, CH], &self.px), opts);
        } else if let Some([x0, y0, x1, y1]) = self.dirty {
            let mut bytes = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
            for y in y0..y1 {
                bytes.extend_from_slice(&self.px[(y * CW + x0) * 4..(y * CW + x1) * 4]);
            }
            tex.set_partial([x0, y0], ColorImage::from_rgba_unmultiplied([x1 - x0, y1 - y0], &bytes), opts);
        }
        self.full = false;
        self.dirty = None;
    }

    fn save(&mut self, ctx: &egui::Context) {
        if self.saving.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let (ctx, px) = (ctx.clone(), self.px.clone());
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Save drawing")
                .add_filter("PNG", &["png"])
                .set_file_name("tilezone.png")
                .save_file();
            let ok = picked.is_some_and(|p| image::save_buffer(p, &px, CW as u32, CH as u32, image::ColorType::Rgba8).is_ok());
            let _ = tx.send(ok);
            ctx.request_repaint();
        });
        self.saving = Some(rx);
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            for (i, rgb) in SWATCHES.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                let selected = !self.erase && self.color == i;
                let r = if resp.hovered() || selected { 8.5 } else { 7.5 };
                ui.painter().circle_filled(rect.center(), r, Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                if selected {
                    ui.painter().circle_stroke(rect.center(), 9.5, Stroke::new(1.5, Color32::WHITE));
                }
                if resp.clicked() {
                    self.color = i;
                    self.erase = false;
                }
            }
        });
        ui.horizontal(|ui| {
            for (i, size) in SIZES.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                let shown = (size.sqrt() * 1.6).min(8.5);
                ui.painter()
                    .circle_filled(rect.center(), shown, dim(if self.size == i { 235 } else { 100 }));
                if resp.clicked() {
                    self.size = i;
                }
            }
            if ui.selectable_label(self.erase, "Erase").clicked() {
                self.erase = !self.erase;
            }
            if ui.small_button("Undo").clicked() {
                if let Some(prev) = self.undo.pop() {
                    self.px = prev;
                    self.full = true;
                    self.unsaved = true;
                }
            }
            if ui.small_button("Clear").clicked() {
                self.snapshot();
                self.px.fill(0);
                self.full = true;
                self.unsaved = true;
            }
            let label = if ui.input(|i| i.time) - self.saved_at < 2.0 { "Saved" } else { "Save" };
            if ui.small_button(label).clicked() {
                self.save(ui.ctx());
            }
        });
    }
}

impl Applet for Paint {
    fn name(&self) -> &'static str {
        "Paint"
    }

    fn kind(&self) -> Kind {
        Kind::Paint
    }

    fn save(&mut self, dir: &Path) -> Option<Value> {
        let name = format!("paint_{:x}.png", self.uid);
        if self.unsaved && self.last.is_none() {
            let file = std::fs::File::create(dir.join(&name)).ok()?;
            let enc = image::codecs::png::PngEncoder::new_with_quality(
                std::io::BufWriter::new(file),
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::NoFilter,
            );
            image::ImageEncoder::write_image(enc, &self.px, CW as u32, CH as u32, image::ExtendedColorType::Rgba8).ok()?;
            self.unsaved = false;
        }
        Some(json!({ "uid": self.uid, "file": name }))
    }

    fn restore(&mut self, _ctx: &egui::Context, dir: &Path, data: &Value) {
        if let Some(uid) = data.get("uid").and_then(|v| v.as_u64()) {
            self.uid = uid;
        }
        let Some(file) = data.get("file").and_then(|v| v.as_str()) else { return };
        if let Ok(img) = image::open(dir.join(file)) {
            let rgba = img.to_rgba8();
            if rgba.width() as usize == CW && rgba.height() as usize == CH {
                self.px = rgba.into_raw();
                self.full = true;
            }
        }
    }

    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        if let Some(rx) = &self.saving {
            if let Ok(ok) = rx.try_recv() {
                self.saving = None;
                if ok {
                    self.saved_at = ui.input(|i| i.time);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(2100));
                }
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
        }

        self.toolbar(ui);
        ui.add_space(4.0);

        let area = ui.available_rect_before_wrap();
        let scale = (area.width() / CW as f32).min(area.height() / CH as f32).max(0.05);
        let canvas = Rect::from_center_size(area.center(), vec2(CW as f32, CH as f32) * scale);
        let resp = ui.interact(canvas, ui.scope_id().with("canvas"), Sense::drag());

        if resp.is_pointer_button_down_on() {
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                let c = pos2((p.x - canvas.min.x) / scale, (p.y - canvas.min.y) / scale);
                let r = SIZES[self.size] / 2.0;
                match self.last {
                    Some(prev) => self.line(prev, c, r),
                    None => {
                        self.snapshot();
                        self.stamp(c, r);
                    }
                }
                self.last = Some(c);
            }
        } else {
            self.last = None;
        }

        self.flush(ui.ctx());
        ui.painter().rect_filled(canvas, 14.0, dim(18));
        if let Some(tex) = &self.tex {
            egui::Image::new((tex.id(), canvas.size())).corner_radius(14.0).paint_at(ui, canvas);
        }
    }
}
