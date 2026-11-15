use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align, Align2, Color32, FontFamily, FontId, Galley, Key, Layout, Painter, Pos2,
    Rect, RichText, Sense, Shape, Stroke, Ui, Vec2,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::{Disks, Gpus, System};

use crate::glass;
use crate::paint::Paint;
use crate::settings::{Hub, Settings};
use crate::shooter::Shooter;
use crate::sprites;
use crate::viewer::Viewer;

pub const ACCENT: Color32 = Color32::from_rgb(192, 96, 255);

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("InterBold".into()))
}

pub trait Applet {
    fn name(&self) -> &'static str;
    fn kind(&self) -> Kind;
    fn titled(&self) -> bool {
        true
    }
    fn save(&mut self, _dir: &Path) -> Option<Value> {
        None
    }
    fn restore(&mut self, _ctx: &egui::Context, _dir: &Path, _data: &Value) {}
    fn ghost(&mut self, ui: &mut Ui) {
        self.ui(ui);
    }
    fn ui(&mut self, ui: &mut Ui);
}

pub fn overlay(p: &Painter, rect: Rect, title: &str, sub: &str) {
    p.rect_filled(rect, 16.0, Color32::from_black_alpha(100));
    let max_w = (rect.width() - 20.0).max(20.0);
    let fit = |text: &str, mut size: f32, font: fn(f32) -> FontId, color: Color32| loop {
        let g = p.layout_no_wrap(text.to_string(), font(size), color);
        if g.size().x <= max_w || size <= 7.5 {
            return g;
        }
        size -= 0.5;
    };
    let a = fit(title, 15.0, bold, Color32::WHITE);
    let b = fit(sub, 12.0, FontId::proportional, dim(190));
    let top = rect.center().y - (a.size().y + 4.0 + b.size().y) / 2.0;
    let (ax, bx) = (rect.center().x - a.size().x / 2.0, rect.center().x - b.size().x / 2.0);
    let ah = a.size().y;
    p.galley(pos2(ax, top), a, Color32::WHITE);
    p.galley(pos2(bx, top + ah + 4.0), b, dim(190));
}

pub fn ghost_fill(ui: &mut Ui) {
    ui.painter().rect_filled(ui.max_rect(), 18.0, dim(26));
}

#[derive(Default)]
pub struct Picker {
    rx: Option<Receiver<Option<PathBuf>>>,
}

impl Picker {
    pub fn open(&mut self, ctx: &egui::Context, title: &'static str, exts: &'static [&'static str]) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new().set_title(title).add_filter("Supported files", exts).pick_file();
            let _ = tx.send(path);
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(path) => {
                self.rx = None;
                path
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(250));
                None
            }
            Err(TryRecvError::Disconnected) => {
                self.rx = None;
                None
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Kind {
    System,
    Todo,
    Notepad,
    Clipboard,
    Photo,
    ClickSpeed,
    Snake,
    Shooter,
    Paint,
    Viewer,
    Settings,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::System,
        Kind::Todo,
        Kind::Notepad,
        Kind::Clipboard,
        Kind::Photo,
        Kind::ClickSpeed,
        Kind::Snake,
        Kind::Shooter,
        Kind::Paint,
        Kind::Viewer,
        Kind::Settings,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::System => "System",
            Kind::Todo => "Reminders",
            Kind::Notepad => "Notes",
            Kind::Clipboard => "Clipboard",
            Kind::Photo => "Photo",
            Kind::ClickSpeed => "Click speed",
            Kind::Snake => "Snake",
            Kind::Shooter => "Space shooter",
            Kind::Paint => "Paint",
            Kind::Viewer => "Viewer",
            Kind::Settings => "Settings",
        }
    }

    pub fn span(self) -> (usize, usize) {
        match self {
            Kind::System | Kind::Settings => (4, 1),
            _ => (2, 2),
        }
    }

    pub fn build(self, hub: &Hub) -> Box<dyn Applet> {
        match self {
            Kind::System => Box::new(SystemStats::new(hub.clone())),
            Kind::Todo => Box::new(Todo::new()),
            Kind::Notepad => Box::new(Notepad::new()),
            Kind::Clipboard => Box::new(Clipboard::new(hub.clone())),
            Kind::Photo => Box::new(Photo::new()),
            Kind::ClickSpeed => Box::new(ClickSpeed::new()),
            Kind::Snake => Box::new(Snake::new(hub.clone())),
            Kind::Shooter => Box::new(Shooter::new(hub.clone())),
            Kind::Paint => Box::new(Paint::new()),
            Kind::Viewer => Box::new(Viewer::new()),
            Kind::Settings => Box::new(Settings::new(hub.clone())),
        }
    }
}

pub fn dim(alpha: u8) -> Color32 {
    Color32::from_white_alpha(alpha)
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
    stroke_arc(p, c, r, w, start, start + sweep, dim(38), caps);
    let f = frac.clamp(0.0, 1.0).max(0.001);
    stroke_arc(p, c, r, w, start, start + sweep * f, dim(235), true);
}

fn gauge(ui: &mut Ui, rect: Rect, label: &str, frac: f32, detail: String) {
    let r = (rect.width().min(rect.height()) / 2.0 - 4.0).max(10.0);
    let w = (r * 0.2).clamp(5.0, 16.0);
    let c = rect.center();
    let p = ui.painter().clone();
    ring(&p, c, r - w / 2.0, w, frac, 0.75 * TAU);
    p.text(
        c + vec2(0.0, -r * 0.06),
        Align2::CENTER_CENTER,
        format!("{:.0}%", frac * 100.0),
        bold((r * 0.4).clamp(13.0, 44.0)),
        Color32::WHITE,
    );
    p.text(
        c + vec2(0.0, r * 0.74),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional((r * 0.22).clamp(10.0, 16.0)),
        dim(190),
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
    hub: Hub,
}

impl SystemStats {
    pub fn new(hub: Hub) -> Self {
        let mut sys = System::new().expect("system info unavailable");
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self {
            sys,
            disks: Disks::new_with_refreshed_list().expect("disk info unavailable"),
            gpus: Gpus::new_with_refreshed_list().ok(),
            last: Instant::now(),
            hub,
        }
    }
}

impl Applet for SystemStats {
    fn name(&self) -> &'static str {
        "System"
    }

    fn kind(&self) -> Kind {
        Kind::System
    }

    fn titled(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut Ui) {
        let every = Duration::from_secs_f32(self.hub.borrow().sys_interval.clamp(0.5, 10.0));
        if self.last.elapsed() >= every {
            self.sys.refresh_cpu_usage();
            self.sys.refresh_memory();
            self.disks.refresh(true);
            if let Some(gpus) = &mut self.gpus {
                gpus.refresh(true);
            }
            self.last = Instant::now();
        }
        ui.ctx().request_repaint_after(every);

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
        let n = items.len();
        let (cols, cell) = (1..=n)
            .map(|c| (c, (rect.width() / c as f32).min(rect.height() / n.div_ceil(c) as f32)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((1, 10.0));
        let rows = n.div_ceil(cols);
        let origin = rect.center() - vec2(cell * cols as f32, cell * rows as f32) / 2.0;

        for (i, (label, frac, detail)) in items.into_iter().enumerate() {
            let (cx, cy) = (i % cols, i / cols);
            let in_row = if cy == rows - 1 { n - cy * cols } else { cols };
            let shift = (cols - in_row) as f32 * cell / 2.0;
            let min = origin + vec2(cx as f32 * cell + shift, cy as f32 * cell);
            gauge(ui, Rect::from_min_size(min, vec2(cell, cell)), label, frac, detail);
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
        p.circle_filled(c, 8.0, dim(235));
        p.add(Shape::line(
            vec![c + vec2(-3.5, 0.5), c + vec2(-1.0, 3.0), c + vec2(3.5, -2.5)],
            Stroke::new(1.8, Color32::from_black_alpha(190)),
        ));
    } else {
        p.circle_stroke(c, 8.0, Stroke::new(1.5, dim(170)));
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

    fn kind(&self) -> Kind {
        Kind::Todo
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        Some(json!({ "items": self.items }))
    }

    fn restore(&mut self, _ctx: &egui::Context, _dir: &Path, data: &Value) {
        if let Some(items) = data.get("items").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            self.items = items;
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        let left = self.items.iter().filter(|(d, _)| !*d).count();
        ui.horizontal(|ui| {
            ui.label(RichText::new(left.to_string()).font(bold(26.0)));
            ui.label(RichText::new(if left == 1 { "task left" } else { "tasks left" }).weak());
        });

        let field = ui.add(
            egui::TextEdit::singleline(&mut self.input)
                .hint_text("New reminder")
                .desired_width(f32::INFINITY),
        );
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.input.trim().is_empty() {
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

    fn kind(&self) -> Kind {
        Kind::Notepad
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        Some(json!({ "text": self.text }))
    }

    fn restore(&mut self, _ctx: &egui::Context, _dir: &Path, data: &Value) {
        if let Some(text) = data.get("text").and_then(|v| v.as_str()) {
            self.text = text.to_string();
        }
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

fn truncated(ui: &Ui, text: &str, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), FontId::proportional(12.5), Color32::WHITE);
    job.wrap.max_width = width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

pub struct Clipboard {
    handle: Option<arboard::Clipboard>,
    items: Vec<String>,
    last: Instant,
    hub: Hub,
}

impl Clipboard {
    pub fn new(hub: Hub) -> Self {
        Self {
            handle: arboard::Clipboard::new().ok(),
            items: Vec::new(),
            last: Instant::now() - Duration::from_secs(1),
            hub,
        }
    }

    fn poll(&mut self) {
        if self.last.elapsed() < Duration::from_millis(400) {
            return;
        }
        self.last = Instant::now();
        let Some(handle) = &mut self.handle else { return };
        if let Ok(text) = handle.get_text() {
            let text = text.trim().to_string();
            if !text.is_empty() && self.items.first() != Some(&text) {
                self.items.retain(|t| *t != text);
                self.items.insert(0, text);
                let keep = self.hub.borrow().clip_len.max(1);
                self.items.truncate(keep);
            }
        }
    }
}

impl Applet for Clipboard {
    fn name(&self) -> &'static str {
        "Clipboard"
    }

    fn kind(&self) -> Kind {
        Kind::Clipboard
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.poll();
        ui.ctx().request_repaint_after(Duration::from_millis(400));

        if self.handle.is_none() {
            ui.label(RichText::new("Clipboard isn't available here").weak());
            return;
        }

        ui.label(RichText::new(format!("{} recent", self.items.len())).weak());

        if self.items.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("Copy something and it shows up here").weak());
        }

        let mut copy = None;
        egui::ScrollArea::vertical()
            .id_salt("clipboard_list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, text) in self.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    let alpha = if resp.is_pointer_button_down_on() {
                        56
                    } else if resp.hovered() {
                        38
                    } else {
                        20
                    };
                    ui.painter().rect_filled(rect, 12.0, dim(alpha));

                    let first = text.lines().next().unwrap_or("");
                    let galley = truncated(ui, first, rect.width() - 20.0);
                    let pos = pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0);
                    ui.painter().galley(pos, galley, Color32::WHITE);

                    if resp.clicked() {
                        copy = Some(i);
                    }
                    let preview: String = text.chars().take(300).collect();
                    resp.on_hover_text(preview);
                }
            });

        if let Some(i) = copy {
            ui.ctx().copy_text(self.items[i].clone());
        }
    }
}

pub struct Photo {
    tex: Option<egui::TextureHandle>,
    err: Option<String>,
    picker: Picker,
    path: Option<PathBuf>,
}

impl Photo {
    pub fn new() -> Self {
        Self { tex: None, err: None, picker: Picker::default(), path: None }
    }

    fn load(&mut self, ctx: &egui::Context, path: &Path) {
        match image::open(path) {
            Ok(img) => {
                let rgba = img.thumbnail(2048, 2048).to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                self.tex = Some(ctx.load_texture("pinned_photo", color, egui::TextureOptions::LINEAR));
                self.err = None;
                self.path = Some(path.to_path_buf());
            }
            Err(e) => self.err = Some(e.to_string()),
        }
    }

    fn choose(&mut self, ctx: &egui::Context) {
        self.picker.open(ctx, "Choose a photo", &["png", "jpg", "jpeg", "webp"]);
    }
}

impl Applet for Photo {
    fn name(&self) -> &'static str {
        "Photo"
    }

    fn kind(&self) -> Kind {
        Kind::Photo
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        self.path.as_ref().map(|p| json!({ "path": p.to_string_lossy() }))
    }

    fn restore(&mut self, ctx: &egui::Context, _dir: &Path, data: &Value) {
        if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
            self.load(ctx, Path::new(path));
            self.err = None;
        }
    }

    fn titled(&self) -> bool {
        false
    }

    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        if let Some(path) = self.picker.poll(ui.ctx()) {
            self.load(ui.ctx(), &path);
        }
        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.scope_id().with("photo"), Sense::click());

        let Some(tex) = &self.tex else {
            ui.painter().rect_filled(rect, 24.0, dim(if resp.hovered() { 26 } else { 16 }));
            let msg = self.err.as_deref().unwrap_or("Choose a photo");
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(13.0), dim(170));
            if resp.clicked() {
                self.choose(ui.ctx());
            }
            return;
        };

        let ts = tex.size_vec2();
        let (ra, ta) = (rect.width() / rect.height(), ts.x / ts.y);
        let uv = if ta > ra {
            let w = ra / ta;
            Rect::from_min_max(pos2((1.0 - w) / 2.0, 0.0), pos2((1.0 + w) / 2.0, 1.0))
        } else {
            let h = ta / ra;
            Rect::from_min_max(pos2(0.0, (1.0 - h) / 2.0), pos2(1.0, (1.0 + h) / 2.0))
        };
        egui::Image::new((tex.id(), ts)).uv(uv).corner_radius(24.0).paint_at(ui, rect);

        let mut change = false;
        glass::context_menu(&resp, |ui| {
            if ui.button("Change photo").clicked() {
                change = true;
                ui.close();
            }
        });
        if change {
            self.choose(ui.ctx());
        }
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

    fn kind(&self) -> Kind {
        Kind::ClickSpeed
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        Some(json!({ "best": self.best, "total": self.total }))
    }

    fn restore(&mut self, _ctx: &egui::Context, _dir: &Path, data: &Value) {
        self.best = data.get("best").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        self.total = data.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
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

        let w = (d * 0.09).clamp(6.0, 20.0);
        let c = sq.center();
        let disc = if resp.is_pointer_button_down_on() {
            46
        } else if resp.hovered() {
            30
        } else {
            18
        };
        let p = ui.painter().clone();
        p.circle_filled(c, d / 2.0 - w, dim(disc));
        ring(&p, c, d / 2.0 - w / 2.0, w, cps as f32 / 10.0, TAU);
        p.text(
            c + vec2(0.0, -d * 0.06),
            Align2::CENTER_CENTER,
            cps.to_string(),
            bold((d * 0.3).clamp(18.0, 120.0)),
            Color32::WHITE,
        );
        p.text(
            c + vec2(0.0, d * 0.18),
            Align2::CENTER_CENTER,
            "clicks / sec",
            FontId::proportional((d * 0.075).clamp(11.0, 18.0)),
            dim(190),
        );
        p.text(
            pos2(rect.center().x, rect.max.y - 8.0),
            Align2::CENTER_CENTER,
            format!("best {}  ·  {} total", self.best, self.total),
            FontId::proportional(11.0),
            dim(160),
        );

        glass::context_menu(&resp, |ui| {
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

const N: i32 = 16;

#[derive(PartialEq)]
enum Phase {
    Idle,
    Running,
    Dead,
}

pub struct Snake {
    body: VecDeque<(i32, i32)>,
    dir: (i32, i32),
    queued: (i32, i32),
    food: (i32, i32),
    phase: Phase,
    score: u32,
    best: u32,
    last: f64,
    rng: u64,
    hub: Hub,
    apple: Option<egui::TextureHandle>,
}

impl Snake {
    pub fn new(hub: Hub) -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
        let mut s = Self {
            body: VecDeque::new(),
            dir: (1, 0),
            queued: (1, 0),
            food: (0, 0),
            phase: Phase::Idle,
            score: 0,
            best: 0,
            last: 0.0,
            rng: seed,
            hub,
            apple: None,
        };
        s.reset();
        s.phase = Phase::Idle;
        s
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as u32
    }

    fn place_food(&mut self) {
        for _ in 0..200 {
            let p = ((self.rand() % N as u32) as i32, (self.rand() % N as u32) as i32);
            if !self.body.contains(&p) {
                self.food = p;
                return;
            }
        }
    }

    fn reset(&mut self) {
        self.body = VecDeque::from([(8, 8), (7, 8), (6, 8)]);
        self.dir = (1, 0);
        self.queued = (1, 0);
        self.score = 0;
        self.phase = Phase::Running;
        self.place_food();
    }

    fn step(&mut self) {
        self.dir = self.queued;
        let head = self.body[0];
        let next = (head.0 + self.dir.0, head.1 + self.dir.1);
        if !(0..N).contains(&next.0) || !(0..N).contains(&next.1) || self.body.contains(&next) {
            self.phase = Phase::Dead;
            self.best = self.best.max(self.score);
            return;
        }
        self.body.push_front(next);
        if next == self.food {
            self.score += 1;
            self.place_food();
        } else {
            self.body.pop_back();
        }
    }

    fn steer(&mut self, to: (i32, i32)) {
        if to.0 != -self.dir.0 || to.1 != -self.dir.1 {
            self.queued = to;
        }
    }
}

impl Applet for Snake {
    fn name(&self) -> &'static str {
        "Snake"
    }

    fn kind(&self) -> Kind {
        Kind::Snake
    }

    fn save(&mut self, _dir: &Path) -> Option<Value> {
        Some(json!({ "best": self.best }))
    }

    fn restore(&mut self, _ctx: &egui::Context, _dir: &Path, data: &Value) {
        self.best = data.get("best").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    }

    fn ui(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let side = rect.width().min(rect.height() - 18.0).max(40.0);
        let board = Rect::from_center_size(pos2(rect.center().x, rect.min.y + side / 2.0), vec2(side, side));
        let resp = ui.interact(board, ui.scope_id().with("board"), Sense::click());

        if resp.clicked() && self.phase != Phase::Running {
            self.reset();
            self.last = ui.input(|i| i.time);
        }
        if resp.hovered() && self.phase == Phase::Running {
            let (up, down, left, right) = ui.input(|i| {
                (
                    i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::W),
                    i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::S),
                    i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::A),
                    i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::D),
                )
            });
            if up {
                self.steer((0, -1));
            } else if down {
                self.steer((0, 1));
            } else if left {
                self.steer((-1, 0));
            } else if right {
                self.steer((1, 0));
            }
        }

        if self.phase == Phase::Running {
            let now = ui.input(|i| i.time);
            let speed = self.hub.borrow().snake_speed.clamp(0.5, 3.0) as f64;
            let interval = (0.14 - self.score as f64 * 0.003).max(0.07) / speed;
            if now - self.last >= interval {
                self.last = now;
                self.step();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        let cell = side / N as f32;
        let p = ui.painter().clone();
        p.rect_filled(board, 16.0, dim(16));

        let len = self.body.len().max(1) as f32;
        for (i, &(x, y)) in self.body.iter().enumerate() {
            let min = board.min + vec2(x as f32, y as f32) * cell;
            let r = Rect::from_min_size(min, vec2(cell, cell)).shrink(cell * 0.06);
            let k = i as f32 / len;
            let color = Color32::from_rgb((110.0 - 60.0 * k) as u8, (235.0 - 90.0 * k) as u8, (120.0 - 40.0 * k) as u8);
            p.rect_filled(r, cell * 0.38, color);
            if i == 0 {
                let d = vec2(self.dir.0 as f32, self.dir.1 as f32);
                let side = vec2(-d.y, d.x);
                for s in [-1.0, 1.0] {
                    let eye = r.center() + d * cell * 0.14 + side * s * cell * 0.17;
                    p.circle_filled(eye, cell * 0.08, Color32::from_black_alpha(220));
                }
            }
        }
        let apple = self
            .apple
            .get_or_insert_with(|| sprites::load(ui.ctx(), "apple", sprites::APPLE, egui::TextureOptions::LINEAR));
        let fc = board.min + vec2(self.food.0 as f32 + 0.5, self.food.1 as f32 + 0.5) * cell;
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        p.image(apple.id(), Rect::from_center_size(fc, vec2(cell * 1.25, cell * 1.25)), uv, Color32::WHITE);

        match self.phase {
            Phase::Idle => overlay(&p, board, "Snake", "Click to play"),
            Phase::Dead => overlay(&p, board, "Game over", "Click to retry"),
            Phase::Running => {}
        }

        p.text(
            pos2(rect.center().x, rect.max.y - 8.0),
            Align2::CENTER_CENTER,
            format!("score {}  ·  best {}", self.score, self.best),
            FontId::proportional(11.0),
            dim(170),
        );
    }
}