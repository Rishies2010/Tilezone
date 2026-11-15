use std::path::{Path, PathBuf};

use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Key, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};

use serde_json::{json, Value};

use crate::applets::{bold, dim, ghost_fill, Applet, Kind, Picker};

const EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

pub struct Viewer {
    files: Vec<PathBuf>,
    index: usize,
    tex: Option<TextureHandle>,
    err: Option<String>,
    zoom: f32,
    pan: Vec2,
    picker: Picker,
}

impl Viewer {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            index: 0,
            tex: None,
            err: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            picker: Picker::default(),
        }
    }

    fn open(&mut self, ctx: &egui::Context, path: &Path) {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXTS.contains(&e.to_lowercase().as_str()))
            })
            .collect();
        files.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_lowercase()));
        self.index = files.iter().position(|p| p == path).unwrap_or(0);
        self.files = files;
        if self.files.is_empty() {
            self.files.push(path.to_path_buf());
        }
        self.show(ctx);
    }

    fn show(&mut self, ctx: &egui::Context) {
        let path = &self.files[self.index];
        match image::open(path) {
            Ok(img) => {
                let rgba = img.thumbnail(3072, 3072).to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                self.tex = Some(ctx.load_texture("viewer_image", color, egui::TextureOptions::LINEAR));
                self.err = None;
            }
            Err(e) => {
                self.tex = None;
                self.err = Some(e.to_string());
            }
        }
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    fn step(&mut self, ctx: &egui::Context, delta: isize) {
        let n = self.files.len() as isize;
        if n > 1 {
            self.index = (self.index as isize + delta).rem_euclid(n) as usize;
            self.show(ctx);
        }
    }
}

fn arrow(ui: &Ui, center: egui::Pos2, dir: f32, hovered: bool) {
    ui.painter().circle_filled(center, 15.0, Color32::from_black_alpha(if hovered { 120 } else { 70 }));
    let s = Stroke::new(1.8, Color32::WHITE);
    let tip = center + vec2(4.0 * dir, 0.0);
    ui.painter().line_segment([tip, tip + vec2(-6.0 * dir, -6.0)], s);
    ui.painter().line_segment([tip, tip + vec2(-6.0 * dir, 6.0)], s);
}

impl Applet for Viewer {
    fn name(&self) -> &'static str {
        "Viewer"
    }

    fn kind(&self) -> Kind {
        Kind::Viewer
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        self.files.get(self.index).map(|p| json!({ "path": p.to_string_lossy() }))
    }

    fn restore(&mut self, ctx: &egui::Context, _dir: &Path, data: &Value) {
        if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
            if Path::new(path).exists() {
                self.open(ctx, Path::new(path));
            }
        }
    }

    fn titled(&self) -> bool {
        false
    }

    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some(path) = self.picker.poll(&ctx) {
            self.open(&ctx, &path);
        }

        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.scope_id().with("viewer"), Sense::click_and_drag());

        let Some(tex) = self.tex.clone() else {
            ui.painter().rect_filled(rect, 24.0, dim(if resp.hovered() { 26 } else { 16 }));
            let msg = self.err.as_deref().unwrap_or("Open an image");
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(13.0), dim(170));
            if resp.clicked() {
                self.picker.open(&ctx, "Open an image", EXTS);
            }
            return;
        };

        if resp.hovered() {
            let (scroll, ctrl, left, right) = ui.input(|i| {
                (
                    i.smooth_scroll_delta.y,
                    i.modifiers.ctrl,
                    i.key_pressed(Key::ArrowLeft),
                    i.key_pressed(Key::ArrowRight),
                )
            });
            if scroll != 0.0 && !ctrl {
                self.zoom = (self.zoom * (scroll * 0.004).exp()).clamp(1.0, 12.0);
            }
            if left {
                self.step(&ctx, -1);
            }
            if right {
                self.step(&ctx, 1);
            }
        }
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.double_clicked() {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }

        let ts = tex.size_vec2();
        let fit = (rect.width() / ts.x).min(rect.height() / ts.y);
        let size = ts * fit * self.zoom;
        let limit = ((size - rect.size()) / 2.0).max(Vec2::ZERO);
        self.pan = self.pan.clamp(-limit, limit);
        let img = Rect::from_center_size(rect.center() + self.pan, size);
        let radius = if self.zoom <= 1.001 { 18.0 } else { 0.0 };
        egui::Image::new((tex.id(), ts)).corner_radius(radius).paint_at(ui, img);

        if resp.hovered() && self.files.len() > 1 {
            let pointer = ui.input(|i| i.pointer.hover_pos());
            let spots = [(rect.left_center() + vec2(20.0, 0.0), -1.0, -1isize), (rect.right_center() - vec2(20.0, 0.0), 1.0, 1isize)];
            for (c, dir, delta) in spots {
                let over = pointer.is_some_and(|p| p.distance(c) < 18.0);
                arrow(ui, c, dir, over);
                if over && resp.clicked() {
                    self.step(&ctx, delta);
                }
            }
            let name = self.files[self.index].file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let caption = format!("{}  ·  {} / {}", name, self.index + 1, self.files.len());
            let pos = pos2(rect.center().x, rect.max.y - 14.0);
            let galley = ui.painter().layout_no_wrap(caption, bold(11.5), Color32::WHITE);
            let pill = Rect::from_center_size(pos, galley.size() + vec2(18.0, 8.0)).intersect(rect);
            ui.painter().rect_filled(pill, 12.0, Color32::from_black_alpha(100));
            ui.painter().galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
        }

        let mut open = false;
        crate::glass::context_menu(&resp, |ui| {
            if ui.button("Open image").clicked() {
                open = true;
                ui.close();
            }
        });
        if open {
            self.picker.open(&ctx, "Open an image", EXTS);
        }
    }
}
