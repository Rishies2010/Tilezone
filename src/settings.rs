use std::cell::RefCell;
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::rc::Rc;

use eframe::egui::{self, vec2, Color32, RichText, Sense, Ui};
use egui_glass::{GlassSlider, GlassStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::applets::{bold, dim, Applet, Kind, Picker, ACCENT};

#[derive(Clone)]
pub enum Choice {
    File(PathBuf),
    Default,
}

pub const SCROLL_MODS: [&str; 4] = ["Ctrl", "Ctrl+Shift", "Shift", "Alt"];

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Prefs {
    pub wallpaper_path: Option<PathBuf>,
    pub refraction: f32,
    pub zoom: f32,
    pub blur: f32,
    pub edge_width: f32,
    pub chromatic: f32,
    pub specular: f32,
    pub saturation: f32,
    pub tint: f32,
    pub corner_radius: f32,
    pub gap: f32,
    pub reduce: bool,
    pub warp: bool,
    pub warp_strength: f32,
    pub hover_fx: bool,
    pub fps: usize,
    pub scroll_mod: usize,
    pub pet: bool,
    pub pet_follow: bool,
    pub pet_scale: f32,
    pub pet_interval: f32,
    pub snake_speed: f32,
    pub shooter_difficulty: f32,
    pub clip_len: usize,
    pub sys_interval: f32,
    #[serde(skip)]
    pub request: Option<Choice>,
    #[serde(skip)]
    pub label: String,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub reset_layout: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            wallpaper_path: None,
            refraction: 60.0,
            zoom: 1.08,
            blur: 2.0,
            edge_width: 50.0,
            chromatic: 0.3,
            specular: 0.9,
            saturation: 1.2,
            tint: 0.0,
            corner_radius: 40.0,
            gap: 12.0,
            reduce: false,
            warp: true,
            warp_strength: 1.0,
            hover_fx: true,
            fps: 60,
            scroll_mod: 0,
            pet: true,
            pet_follow: true,
            pet_scale: 2.0,
            pet_interval: 15.0,
            snake_speed: 1.0,
            shooter_difficulty: 1.0,
            clip_len: 30,
            sys_interval: 1.0,
            request: None,
            label: String::new(),
            error: None,
            reset_layout: false,
        }
    }
}

impl Prefs {
    pub fn glass(&self, mut style: GlassStyle) -> GlassStyle {
        style.refraction = self.refraction;
        style.zoom = self.zoom;
        style.blur = self.blur;
        style.edge_width = self.edge_width;
        style.chromatic = self.chromatic;
        style.specular = self.specular;
        style.saturation = self.saturation;
        style.corner_radius = self.corner_radius;
        style.tint = Color32::from_rgba_unmultiplied(255, 255, 255, (self.tint * 255.0) as u8);
        style
    }

    pub fn reset(&mut self) {
        let keep = self.wallpaper_path.take();
        let (label, error) = (std::mem::take(&mut self.label), self.error.take());
        *self = Self::default();
        self.wallpaper_path = keep;
        self.label = label;
        self.error = error;
    }
}

pub type Hub = Rc<RefCell<Prefs>>;

pub fn hub(prefs: Prefs) -> Hub {
    Rc::new(RefCell::new(prefs))
}

const MEDIA: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "mp4", "mkv", "webm", "mov", "avi"];

pub struct Settings {
    hub: Hub,
    picker: Picker,
}

impl Settings {
    pub fn new(hub: Hub) -> Self {
        Self { hub, picker: Picker::default() }
    }

    fn background(&mut self, ui: &mut Ui, compact: bool) {
        let (label, error) = {
            let p = self.hub.borrow();
            (p.label.clone(), p.error.clone())
        };
        let status = match error {
            Some(e) => RichText::new(e).color(Color32::from_rgb(255, 150, 160)),
            None => RichText::new(label).weak(),
        };
        let buttons = |this: &mut Self, ui: &mut Ui| {
            if ui.button("Choose file").clicked() {
                this.picker.open(ui.ctx(), "Choose a background", MEDIA);
            }
            if ui.button("Default").clicked() {
                this.hub.borrow_mut().request = Some(Choice::Default);
            }
        };
        if compact {
            ui.horizontal(|ui| {
                buttons(self, ui);
                ui.add(egui::Label::new(status).truncate());
            });
        } else {
            ui.add(egui::Label::new(status).truncate());
            ui.horizontal(|ui| buttons(self, ui));
        }
    }

    fn visuals(&mut self, ui: &mut Ui) {
        section(ui, "Background");
        self.background(ui, false);

        section(ui, "Glass");
        let mut p = self.hub.borrow_mut();
        slider(ui, "Refraction", &mut p.refraction, 0.0..=100.0);
        slider(ui, "Zoom", &mut p.zoom, 1.0..=1.3);
        slider(ui, "Blur", &mut p.blur, 0.0..=30.0);
        slider(ui, "Edge width", &mut p.edge_width, 10.0..=100.0);
        slider(ui, "Dispersion", &mut p.chromatic, 0.0..=1.0);
        slider(ui, "Specular", &mut p.specular, 0.0..=1.0);
        slider(ui, "Saturation", &mut p.saturation, 0.5..=2.0);
        slider(ui, "Frost", &mut p.tint, 0.0..=0.4);
        slider(ui, "Corner radius", &mut p.corner_radius, 16.0..=64.0);
    }

    fn behaviour(&mut self, ui: &mut Ui) {
        let mut p = self.hub.borrow_mut();

        section(ui, "Effects");
        toggle(ui, &mut p.reduce, "Reduce effects");
        toggle(ui, &mut p.warp, "Click warp");
        slider(ui, "Warp strength", &mut p.warp_strength, 0.3..=2.0);
        toggle(ui, &mut p.hover_fx, "Hover glow");
        pills(ui, "Frame rate", &mut p.fps, &[30, 60], |v| format!("{v} fps"));

        section(ui, "Controls");
        ui.horizontal_wrapped(|ui| {
            ui.label("Applet scroll");
            for (i, name) in SCROLL_MODS.iter().enumerate() {
                if ui.selectable_label(p.scroll_mod == i, *name).clicked() {
                    p.scroll_mod = i;
                }
            }
        });

        section(ui, "Layout");
        slider(ui, "Tile gap", &mut p.gap, 4.0..=24.0);

        section(ui, "Pet");
        toggle(ui, &mut p.pet, "Show pet");
        toggle(ui, &mut p.pet_follow, "Visits the cursor");
        slider(ui, "Size", &mut p.pet_scale, 1.0..=4.0);
        slider(ui, "Visit interval (s)", &mut p.pet_interval, 5.0..=60.0);

        section(ui, "Applets");
        slider(ui, "Snake speed", &mut p.snake_speed, 0.5..=2.0);
        slider(ui, "Shooter difficulty", &mut p.shooter_difficulty, 0.5..=2.0);
        let mut len = p.clip_len as f32;
        slider(ui, "Clipboard history", &mut len, 5.0..=100.0);
        p.clip_len = len.round() as usize;
        slider(ui, "System refresh (s)", &mut p.sys_interval, 0.5..=5.0);

        section(ui, "Reset");
        ui.horizontal(|ui| {
            if ui.button("Reset layout").clicked() {
                p.reset_layout = true;
            }
            if ui.button("Reset settings").clicked() {
                p.reset();
            }
        });
    }
}

impl Applet for Settings {
    fn name(&self) -> &'static str {
        "Settings"
    }

    fn kind(&self) -> Kind {
        Kind::Settings
    }

    fn save(&mut self, _dir: &std::path::Path) -> Option<Value> {
        None
    }

    fn ui(&mut self, ui: &mut Ui) {
        if let Some(path) = self.picker.poll(ui.ctx()) {
            self.hub.borrow_mut().request = Some(Choice::File(path));
        }

        if ui.available_height() < 120.0 {
            ui.horizontal_centered(|ui| {
                self.background(ui, true);
                ui.add_space(20.0);
                let mut p = self.hub.borrow_mut();
                toggle(ui, &mut p.reduce, "Reduce effects");
                toggle(ui, &mut p.pet, "Pet");
            });
            return;
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if ui.available_width() > 560.0 {
                ui.columns(2, |cols| {
                    self.visuals(&mut cols[0]);
                    self.behaviour(&mut cols[1]);
                });
            } else {
                self.visuals(ui);
                self.behaviour(ui);
            }
            ui.add_space(8.0);
        });
    }
}

fn section(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).font(bold(12.5)).color(dim(200)));
    ui.add_space(2.0);
}

fn slider(ui: &mut Ui, label: &str, value: &mut f32, range: RangeInclusive<f32>) {
    GlassSlider::new(value, range).text(label).width(ui.available_width()).show(ui);
}

fn pills(ui: &mut Ui, label: &str, current: &mut usize, options: &[usize], show: impl Fn(usize) -> String) {
    ui.horizontal(|ui| {
        ui.label(label);
        for &o in options {
            if ui.selectable_label(*current == o, show(o)).clicked() {
                *current = o;
            }
        }
    });
}

fn toggle(ui: &mut Ui, on: &mut bool, label: &str) {
    ui.horizontal(|ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
        if resp.clicked() {
            *on = !*on;
        }
        let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
        let track = Color32::from_white_alpha(46).lerp_to_gamma(ACCENT, t);
        ui.painter().rect_filled(rect, 10.0, track);
        let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), 8.0, Color32::WHITE);
        ui.label(label);
    });
}
