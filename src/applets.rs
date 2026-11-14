use std::collections::VecDeque;
use std::time::{Duration, Instant};

use eframe::egui::{self, vec2, Align, Align2, Color32, FontId, Layout, RichText, Sense, Ui};
use sysinfo::{Disks, Gpus, System};

pub trait Applet {
    fn name(&self) -> &'static str;
    fn ui(&mut self, ui: &mut Ui);
}

fn meter(ui: &mut Ui, label: &str, frac: f32, value: String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(value);
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, Color32::from_white_alpha(28));
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 4.0, Color32::from_white_alpha(210));
    ui.add_space(12.0);
}

fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

pub struct SystemStats {
    sys: System,
    disks: Disks,
    gpus: Option<Gpus>,
    last: Instant,
}

impl SystemStats {
    pub fn new() -> Self {
        let mut sys = System::new().expect("system info unavailable");
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            disks: Disks::new_with_refreshed_list().expect("disk info unavailable"),
            gpus: Gpus::new_with_refreshed_list().ok(),
            last: Instant::now(),
        }
    }
}

impl Applet for SystemStats {
    fn name(&self) -> &'static str {
        "System"
    }

    fn ui(&mut self, ui: &mut Ui) {
        if self.last.elapsed() >= Duration::from_secs(1) {
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory();
            self.disks.refresh(true);
            if let Some(gpus) = &mut self.gpus {
                gpus.refresh(true);
            }
            self.last = Instant::now();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        let cpu = self.sys.global_cpu_usage() / 100.0;
        meter(ui, "CPU", cpu, format!("{:.0}%", cpu * 100.0));

        let (used, total) = (self.sys.used_memory(), self.sys.total_memory());
        meter(
            ui,
            "RAM",
            used as f32 / total.max(1) as f32,
            format!("{:.1} / {:.1} GB", gb(used), gb(total)),
        );

        if let Some(gpu) = self.gpus.as_ref().and_then(|g| g.iter().next()) {
            if let Some(usage) = gpu.usage() {
                meter(ui, "GPU", usage / 100.0, format!("{usage:.0}%"));
            }
            if let (Some(used), Some(total)) = (gpu.used_memory(), gpu.total_memory()) {
                meter(
                    ui,
                    "VRAM",
                    used as f32 / total.max(1) as f32,
                    format!("{:.1} / {:.1} GB", gb(used), gb(total)),
                );
            }
        }

        if let Some(disk) = self.disks.iter().max_by_key(|d| d.total_space()) {
            let total = disk.total_space();
            let used = total.saturating_sub(disk.available_space());
            let frac = used as f32 / total.max(1) as f32;
            meter(ui, "Disk", frac, format!("{:.0}%", frac * 100.0));
        }
    }
}

pub struct Todo {
    items: Vec<(bool, String)>,
    input: String,
}

impl Todo {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            input: String::new(),
        }
    }
}

impl Applet for Todo {
    fn name(&self) -> &'static str {
        "Todo"
    }

    fn ui(&mut self, ui: &mut Ui) {
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.input)
                .hint_text("add a task")
                .desired_width(f32::INFINITY),
        );
        if field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !self.input.trim().is_empty()
        {
            self.items.push((false, self.input.trim().to_string()));
            self.input.clear();
            field.request_focus();
        }
        ui.add_space(6.0);

        let mut remove = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, (done, text)) in self.items.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(done, "");
                    let label = RichText::new(text.as_str());
                    ui.label(if *done { label.strikethrough().weak() } else { label });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("x").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = remove {
            self.items.remove(i);
        }
    }
}

pub struct Notepad {
    text: String,
}

impl Notepad {
    pub fn new() -> Self {
        Self { text: String::new() }
    }
}

impl Applet for Notepad {
    fn name(&self) -> &'static str {
        "Notepad"
    }

    fn ui(&mut self, ui: &mut Ui) {
        let row = ui.text_style_height(&egui::TextStyle::Body);
        let rows = ((ui.available_height() / row) as usize).saturating_sub(1).max(2);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.text)
                    .hint_text("jot something down")
                    .desired_width(f32::INFINITY)
                    .desired_rows(rows),
            );
        });
    }
}

pub struct ClickSpeed {
    clicks: VecDeque<Instant>,
    total: u32,
    best: usize,
}

impl ClickSpeed {
    pub fn new() -> Self {
        Self {
            clicks: VecDeque::new(),
            total: 0,
            best: 0,
        }
    }
}

impl Applet for ClickSpeed {
    fn name(&self) -> &'static str {
        "Click speed"
    }

    fn ui(&mut self, ui: &mut Ui) {
        let now = Instant::now();
        while self
            .clicks
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1))
        {
            self.clicks.pop_front();
        }
        let cps = self.clicks.len();
        self.best = self.best.max(cps);

        ui.horizontal(|ui| {
            ui.label(RichText::new(cps.to_string()).size(44.0));
            ui.label("clicks/sec");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("reset").clicked() {
                    self.total = 0;
                    self.best = 0;
                }
            });
        });
        ui.label(format!("best {}  |  total {}", self.best, self.total));
        ui.add_space(8.0);

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        let alpha = if resp.is_pointer_button_down_on() {
            60
        } else if resp.hovered() {
            40
        } else {
            24
        };
        ui.painter().rect_filled(rect, 24.0, Color32::from_white_alpha(alpha));
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "click here",
            FontId::proportional(16.0),
            Color32::from_white_alpha(150),
        );

        if resp.contains_pointer() && ui.input(|i| i.pointer.primary_pressed()) {
            self.clicks.push_back(now);
            self.total += 1;
        }
        if !self.clicks.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(60));
        }
    }
}