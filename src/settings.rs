use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use eframe::egui::{self, vec2, Color32, RichText, Sense, Ui};
use egui_glass::GlassSlider;

use crate::applets::{bold, dim, Applet, Picker, ACCENT};

pub enum Choice {
    File(PathBuf),
    Aurora,
}

pub struct Prefs {
    pub wallpaper: Option<Choice>,
    pub label: String,
    pub error: Option<String>,
    pub reduce: bool,
    pub pet: bool,
    pub zoom: f32,
    pub refraction: f32,
}

pub type Hub = Rc<RefCell<Prefs>>;

pub fn hub() -> Hub {
    Rc::new(RefCell::new(Prefs {
        wallpaper: None,
        label: String::new(),
        error: None,
        reduce: false,
        pet: true,
        zoom: 1.08,
        refraction: 60.0,
    }))
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

    fn background(&mut self, ui: &mut Ui) {
        let (label, error) = {
            let p = self.hub.borrow();
            (p.label.clone(), p.error.clone())
        };
        ui.vertical(|ui| {
            ui.label(RichText::new("Background").font(bold(12.5)).color(dim(210)));
            match error {
                Some(e) => ui.add(egui::Label::new(RichText::new(e).color(Color32::from_rgb(255, 150, 160))).truncate()),
                None => ui.add(egui::Label::new(RichText::new(label).weak()).truncate()),
            };
            ui.horizontal(|ui| {
                if ui.button("Choose file").clicked() {
                    self.picker.open(ui.ctx(), "Choose a background", MEDIA);
                }
                if ui.button("Aurora").clicked() {
                    self.hub.borrow_mut().wallpaper = Some(Choice::Aurora);
                }
            });
        });
    }

    fn sliders(&mut self, ui: &mut Ui, width: f32) {
        let (mut refraction, mut zoom) = {
            let p = self.hub.borrow();
            (p.refraction, p.zoom)
        };
        GlassSlider::new(&mut refraction, 0.0..=100.0).text("Refraction").width(width).show(ui);
        GlassSlider::new(&mut zoom, 1.0..=1.3).text("Zoom").width(width).show(ui);
        let mut p = self.hub.borrow_mut();
        p.refraction = refraction;
        p.zoom = zoom;
    }

    fn toggles(&mut self, ui: &mut Ui) {
        let (mut reduce, mut pet) = {
            let p = self.hub.borrow();
            (p.reduce, p.pet)
        };
        toggle(ui, &mut reduce, "Reduce effects");
        toggle(ui, &mut pet, "Pet");
        let mut p = self.hub.borrow_mut();
        p.reduce = reduce;
        p.pet = pet;
    }
}

impl Applet for Settings {
    fn name(&self) -> &'static str {
        "Settings"
    }

    fn titled(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut Ui) {
        if let Some(path) = self.picker.poll(ui.ctx()) {
            self.hub.borrow_mut().wallpaper = Some(Choice::File(path));
        }

        if ui.available_height() < 150.0 {
            ui.horizontal_centered(|ui| {
                self.background(ui);
                ui.add_space(28.0);
                ui.vertical(|ui| self.toggles(ui));
            });
        } else {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new("Settings").font(bold(17.0)));
                ui.add_space(6.0);
                self.background(ui);
                ui.add_space(8.0);
                let width = ui.available_width();
                self.sliders(ui, width);
                ui.add_space(4.0);
                self.toggles(ui);
            });
        }
    }
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
