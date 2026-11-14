use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, FontFamily, FontId, Layout, Painter, Pos2, Rect, RichText, Sense, Shape,
    Stroke, Ui, Vec2,
};
use sysinfo::{Disks, Gpus, System};

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("InterBold".into()))
}

pub trait Applet {
    fn name(&self) -> &'static str;
    fn titled(&self) -> bool {
        true
    }
    fn ui(&mut self, ui: &mut Ui);
}

#[derive(Clone, Copy)]
pub enum Kind {
    System,
    Todo,
    Notepad,
    ClickSpeed,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::System, Kind::Todo, Kind::Notepad, Kind::ClickSpeed];

    pub fn name(self) -> &'static str {
        match self {
            Kind::System => "System",
            Kind::Todo => "Todo",
            Kind::Notepad => "Notepad",
            Kind::ClickSpeed => "Click speed",
        }
    }

    pub fn span(self) -> (usize, usize) {
        match self {
            Kind::System => (3, 1),
            _ => (2, 2),
        }
    }

    pub fn build(self) -> Box<dyn Applet> {
        match self {
            Kind::System => Box::new(SystemStats::new()),
            Kind::Todo => Box::new(Todo::new()),
            Kind::Notepad => Box::new(Notepad::new()),
            Kind::ClickSpeed => Box::new(ClickSpeed::new()),
        }
    }
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = (((a1 - a0).abs() * r / 3.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

fn cap(p: &Painter, e: Pos2, tangent: Vec2, w: f32, color: Color32) {
    let n = vec2(-tangent.y, tangent.x);
    let pts = (0..=12)
        .map(|i| {
            let th = -FRAC_PI_2 + PI * i as f32 / 12.0;
            e + (tangent * th.cos() + n * th.sin()) * (w / 2.0)
        })
        .collect();
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

#[allow(clippy::too_many_arguments)]
fn stroke_arc(p: &Painter, c: Pos2, r: f32, w: f32, a0: f32, a1: f32, color: Color32, caps: bool) {
    p.add(Shape::line(arc(c, r, a0, a1), Stroke::new(w, color)));
    if caps {
        let at = |a: f32| c + vec2(a.cos(), a.sin()) * r;
        cap(p, at(a0), vec2(a0.sin(), -a0.cos()), w, color);
        cap(p, at(a1), vec2(-a1.sin(), a1.cos()), w, color);
    }
}

pub fn ring(p: &Painter, c: Pos2, r: f32, w: f32, frac: f32, sweep: f32) {
    let start = if sweep >= TAU { -FRAC_PI_2 } else { FRAC_PI_2 + (TAU - sweep) / 2.0 };
    let caps = sweep < TAU;
    stroke_arc(p, c, r, w, start, start + sweep, Color32::from_white_alpha(38), caps);
    let f = frac.clamp(0.0, 1.0).max(0.001);
    stroke_arc(p, c, r, w, start, start + sweep * f, Color32::from_white_alpha(235), true);
}

fn gauge(ui: &mut Ui, rect: Rect, label: &str, frac: f32, detail: String) {
    let r = (rect.width().min(rect.height()) / 2.0 - 4.0).max(10.0);
    let w = (r * 0.2).clamp(5.0, 12.0);
    let c = rect.center();
    let p = ui.painter().clone();
    ring(&p, c, r - w / 2.0, w, frac, 0.75 * TAU);
    p.text(
        c + vec2(0.0, -r * 0.06),
        Align2::CENTER_CENTER,
        format!("{:.0}%", frac * 100.0),
        bold((r * 0.4).clamp(13.0, 28.0)),
        Color32::WHITE,
    );
    p.text(
        c + vec2(0.0, r * 0.74),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        Color32::from_white_alpha(190),
    );
    ui.interact(rect, ui.scope_id().with(label), Sense::hover()).on_hover_text(detail);
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

    fn titled(&self) -> bool {
        false
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
        let (used, total) = (self.sys.used_memory(), self.sys.total_memory());
        let mut items = vec![
            ("CPU", cpu, format!("CPU {:.0}%", cpu * 100.0)),
            (
                "RAM",
                used as f32 / total.max(1) as f32,
                format!("{:.1} / {:.1} GB", gb(used), gb(total)),
            ),
        ];

        if let Some(usage) = self.gpus.as_ref().and_then(|g| g.iter().next()).and_then(|g| g.usage()) {
            items.push(("GPU", usage / 100.0, format!("GPU {usage:.0}%")));
        }

        if let Some(disk) = self.disks.iter().max_by_key(|d| d.total_space()) {
            let total = disk.total_space();
            let used = total.saturating_sub(disk.available_space());
            items.push((
                "Disk",
                used as f32 / total.max(1) as f32,
                format!("{:.0} / {:.0} GB", gb(used), gb(total)),
            ));
        }

        let rect = ui.max_rect();
        let cell_w = rect.width() / items.len() as f32;
        for (i, (label, frac, detail)) in items.into_iter().enumerate() {
            let cell = Rect::from_min_size(rect.min + vec2(i as f32 * cell_w, 0.0), vec2(cell_w, rect.height()));
            gauge(ui, cell, label, frac, detail);
        }
    }
}

fn check(ui: &mut Ui, done: &mut bool) {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    if resp.clicked() {
        *done = !*done;
    }
    let c = rect.center();
    let p = ui.painter();
    if *done {
        p.circle_filled(c, 8.0, Color32::from_white_alpha(235));
        p.add(Shape::line(
            vec![c + vec2(-3.5, 0.5), c + vec2(-1.0, 3.0), c + vec2(3.5, -2.5)],
            Stroke::new(1.8, Color32::from_black_alpha(190)),
        ));
    } else {
        p.circle_stroke(c, 8.0, Stroke::new(1.5, Color32::from_white_alpha(170)));
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
        "Reminders"
    }

    fn ui(&mut self, ui: &mut Ui) {
        let left = self.items.iter().filter(|(d, _)| !*d).count();
        ui.horizontal(|ui| {
            ui.label(RichText::new(left.to_string()).font(bold(30.0)));
            ui.label(RichText::new(if left == 1 { "task left" } else { "tasks left" }).weak());
        });

        let field = ui.add(
            egui::TextEdit::singleline(&mut self.input)
                .hint_text("New reminder")
                .desired_width(f32::INFINITY),
        );
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.input.trim().is_empty() {
            self.items.push((false, self.input.trim().to_string()));
            self.input.clear();
            field.request_focus();
        }
        ui.add_space(4.0);

        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("todo_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, (done, text)) in self.items.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        check(ui, done);
                        let label = RichText::new(text.as_str());
                        ui.label(if *done { label.strikethrough().weak() } else { label });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let x = egui::Label::new(RichText::new("×").weak()).sense(Sense::click());
                            if ui.add(x).clicked() {
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
        "Notes"
    }

    fn ui(&mut self, ui: &mut Ui) {
        let row = ui.text_style_height(&egui::TextStyle::Body);
        let rows = ((ui.available_height() / row) as usize).saturating_sub(1).max(2);
        egui::ScrollArea::vertical()
            .id_salt("notepad_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .frame(egui::Frame::NONE)
                        .hint_text("Jot something down")
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

        let rect = ui.max_rect();
        let area = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - 20.0));
        let d = area.width().min(area.height());
        let sq = Rect::from_center_size(area.center(), vec2(d, d));

        let resp = ui.interact(sq, ui.scope_id().with("tap"), Sense::click());
        if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
            self.clicks.push_back(now);
            self.total += 1;
        }

        let w = (d * 0.09).clamp(6.0, 14.0);
        let c = sq.center();
        let disc = if resp.is_pointer_button_down_on() {
            46
        } else if resp.hovered() {
            30
        } else {
            18
        };
        let p = ui.painter().clone();
        p.circle_filled(c, d / 2.0 - w, Color32::from_white_alpha(disc));
        ring(&p, c, d / 2.0 - w / 2.0, w, cps as f32 / 10.0, TAU);
        p.text(
            c + vec2(0.0, -d * 0.06),
            Align2::CENTER_CENTER,
            cps.to_string(),
            bold((d * 0.3).clamp(18.0, 44.0)),
            Color32::WHITE,
        );
        p.text(
            c + vec2(0.0, d * 0.18),
            Align2::CENTER_CENTER,
            "clicks / sec",
            FontId::proportional(11.0),
            Color32::from_white_alpha(190),
        );
        p.text(
            pos2(rect.center().x, rect.max.y - 8.0),
            Align2::CENTER_CENTER,
            format!("best {}  ·  {} total", self.best, self.total),
            FontId::proportional(11.0),
            Color32::from_white_alpha(160),
        );

        resp.context_menu(|ui| {
            if ui.button("Reset").clicked() {
                self.total = 0;
                self.best = 0;
                ui.close();
            }
        });

        if !self.clicks.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(60));
        }
    }
}