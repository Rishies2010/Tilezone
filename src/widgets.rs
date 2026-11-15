use std::f32::consts::{FRAC_PI_2, TAU};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, sync_channel, Receiver};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, ColorImage, FontId, Key, Painter, Pos2, Rect, RichText,
    Sense, Shape, Stroke, TextureHandle, TextureOptions, Ui,
};
use serde_json::{json, Value};

use crate::applets::{bold, dim, ghost_fill, ring, Applet, Kind, Picker, ACCENT};

fn local_offset() -> i64 {
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(|| {
        Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() < 5 {
                    return None;
                }
                let sign = if s.starts_with('-') { -1 } else { 1 };
                let h: i64 = s[1..3].parse().ok()?;
                let m: i64 = s[3..5].parse().ok()?;
                Some(sign * (h * 3600 + m * 60))
            })
            .unwrap_or(0)
    })
}

#[derive(Clone, Copy)]
struct Dt {
    y: i64,
    m: i64,
    d: i64,
    h: i64,
    mi: i64,
    s: i64,
    frac: f32,
    wd: i64,
    day: i64,
}

fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn month_len(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}

fn now_at(offset: i64) -> Dt {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = t.as_secs() as i64 + offset;
    let day = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil(day);
    Dt {
        y,
        m,
        d,
        h: sod / 3600,
        mi: sod % 3600 / 60,
        s: sod % 60,
        frac: t.subsec_millis() as f32 / 1000.0,
        wd: (day + 4).rem_euclid(7),
        day,
    }
}

fn now() -> Dt {
    now_at(local_offset())
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn pill(ui: &mut Ui, rect: Rect, label: &str, on: bool) -> bool {
    let resp = ui.interact(rect, ui.scope_id().with(("pill", label)), Sense::click());
    let a = if on {
        70
    } else if resp.is_pointer_button_down_on() {
        56
    } else if resp.hovered() {
        40
    } else {
        22
    };
    ui.painter().rect_filled(rect, rect.height() / 2.0, dim(a));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.5),
        Color32::WHITE,
    );
    resp.clicked()
}

pub struct Clock {
    mode: usize,
}

impl Clock {
    pub fn new() -> Self {
        Self { mode: 0 }
    }
}

impl Applet for Clock {
    fn name(&self) -> &'static str {
        "Clock"
    }
    fn kind(&self) -> Kind {
        Kind::Clock
    }
    fn titled(&self) -> bool {
        false
    }
    fn save(&mut self, _d: &Path) -> Option<Value> {
        Some(json!({ "mode": self.mode }))
    }
    fn restore(&mut self, _c: &egui::Context, _d: &Path, data: &Value) {
        self.mode = data.get("mode").and_then(|v| v.as_u64()).unwrap_or(0) as usize % 3;
    }
    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        ui.ctx()
            .request_repaint_after(Duration::from_millis(if self.mode == 0 { 33 } else { 250 }));
        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.scope_id().with("clock"), Sense::click());
        if resp.clicked() {
            self.mode = (self.mode + 1) % 3;
        }
        let t = now();
        let p = ui.painter().clone();
        let date = format!(
            "{}, {} {}",
            DAYS[t.wd as usize],
            t.d,
            MONTHS[(t.m - 1) as usize]
        );

        match self.mode {
            0 => {
                let room = if rect.height() > 150.0 { 18.0 } else { 0.0 };
                let area = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.max.y - room));
                let r = (area.width().min(area.height()) / 2.0 - 4.0).max(10.0);
                let c = area.center();
                p.circle_filled(c, r, dim(14));
                p.circle_stroke(c, r, Stroke::new(1.5, dim(70)));
                for i in 0..60 {
                    let a = i as f32 / 60.0 * TAU - FRAC_PI_2;
                    let d = vec2(a.cos(), a.sin());
                    let big = i % 5 == 0;
                    let len = if big { r * 0.12 } else { r * 0.05 };
                    p.line_segment(
                        [c + d * (r - len - 4.0), c + d * (r - 4.0)],
                        Stroke::new(if big { 2.0 } else { 1.0 }, dim(if big { 220 } else { 90 })),
                    );
                }
                let hand = |frac: f32, len: f32, w: f32, col: Color32| {
                    let a = frac * TAU - FRAC_PI_2;
                    let d = vec2(a.cos(), a.sin());
                    p.line_segment([c - d * len * 0.12, c + d * len], Stroke::new(w, col));
                };
                let sec = t.s as f32 + t.frac;
                let min = t.mi as f32 + sec / 60.0;
                let hour = (t.h % 12) as f32 + min / 60.0;
                hand(hour / 12.0, r * 0.5, 4.0, Color32::WHITE);
                hand(min / 60.0, r * 0.75, 3.0, Color32::WHITE);
                hand(sec / 60.0, r * 0.85, 1.6, ACCENT);
                p.circle_filled(c, 4.0, ACCENT);
                if room > 0.0 {
                    p.text(
                        pos2(rect.center().x, rect.max.y - 8.0),
                        Align2::CENTER_CENTER,
                        date,
                        FontId::proportional(11.5),
                        dim(180),
                    );
                }
            }
            1 => {
                let size = (rect.width() / 4.2).min(rect.height() / 2.2).max(18.0);
                let c = rect.center();
                p.text(
                    c + vec2(0.0, -size * 0.15),
                    Align2::CENTER_CENTER,
                    format!("{:02}:{:02}", t.h, t.mi),
                    bold(size),
                    Color32::WHITE,
                );
                p.text(
                    c + vec2(0.0, size * 0.55),
                    Align2::CENTER_CENTER,
                    format!("{:02}s  ·  {}", t.s, date),
                    FontId::proportional((size * 0.28).clamp(10.0, 15.0)),
                    dim(180),
                );
            }
            _ => {
                let digits = [t.h / 10, t.h % 10, t.mi / 10, t.mi % 10, t.s / 10, t.s % 10];
                let cell = (rect.width() / 7.0).min(rect.height() / 5.5).max(8.0);
                let origin = rect.center() - vec2(cell * 6.0, cell * 4.0) / 2.0;
                for (col, dg) in digits.iter().enumerate() {
                    let gap = if col >= 2 { cell * 0.5 } else { 0.0 }
                        + if col >= 4 { cell * 0.5 } else { 0.0 };
                    for bit in 0..4 {
                        let on = (dg >> (3 - bit)) & 1 == 1;
                        let at = origin
                            + vec2(
                                col as f32 * cell + gap - cell * 0.5,
                                bit as f32 * cell + cell / 2.0,
                            );
                        p.circle_filled(at, cell * 0.32, if on { ACCENT } else { dim(26) });
                    }
                }
                p.text(
                    pos2(rect.center().x, rect.max.y - 6.0),
                    Align2::CENTER_CENTER,
                    "tap to switch",
                    FontId::proportional(10.5),
                    dim(110),
                );
            }
        }
    }
}

pub struct Calendar {
    ym: Option<(i64, i64)>,
}

impl Calendar {
    pub fn new() -> Self {
        Self { ym: None }
    }
}

impl Applet for Calendar {
    fn name(&self) -> &'static str {
        "Calendar"
    }
    fn kind(&self) -> Kind {
        Kind::Calendar
    }
    fn ui(&mut self, ui: &mut Ui) {
        let t = now();
        let (y, m) = self.ym.unwrap_or((t.y, t.m));
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {}", MONTHS[(m - 1) as usize], y)).font(bold(14.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut go = 0;
                if ui.small_button("›").clicked() {
                    go = 1;
                }
                if ui.small_button("Today").clicked() {
                    self.ym = None;
                }
                if ui.small_button("‹").clicked() {
                    go = -1;
                }
                if go != 0 {
                    let n = y * 12 + (m - 1) + go;
                    self.ym = Some((n.div_euclid(12), n.rem_euclid(12) + 1));
                }
            });
        });
        let area = ui.available_rect_before_wrap();
        let (cw, ch) = (area.width() / 7.0, area.height() / 7.0);
        let p = ui.painter().clone();
        for (i, n) in DAYS.iter().enumerate() {
            p.text(
                area.min + vec2((i as f32 + 0.5) * cw, ch * 0.5),
                Align2::CENTER_CENTER,
                &n[..2],
                FontId::proportional(11.0),
                dim(130),
            );
        }
        let first = (days_from_civil(y, m, 1) + 4).rem_euclid(7);
        for d in 1..=month_len(y, m) {
            let idx = first + d - 1;
            let (col, row) = (idx % 7, idx / 7 + 1);
            let c = area.min + vec2((col as f32 + 0.5) * cw, (row as f32 + 0.5) * ch);
            let today = (y, m, d) == (t.y, t.m, t.d);
            if today {
                p.circle_filled(c, (cw.min(ch) * 0.42).max(6.0), ACCENT);
            }
            p.text(
                c,
                Align2::CENTER_CENTER,
                d.to_string(),
                FontId::proportional(12.5),
                if today {
                    Color32::WHITE
                } else if col == 0 {
                    dim(150)
                } else {
                    dim(225)
                },
            );
        }
        ui.allocate_rect(area, Sense::hover());
    }
}

struct Wx {
    city: String,
    temp: f64,
    code: i64,
    wind: f64,
    hum: f64,
    hi: f64,
    lo: f64,
}

enum WState {
    Idle,
    Loading(Receiver<Result<Wx, String>>),
    Ready(Wx, Instant),
    Failed(String, Instant),
}

fn curl(url: &str) -> Result<Value, String> {
    let o = Command::new("curl")
        .args(["-sS", "-m", "8", url])
        .output()
        .map_err(|_| "curl not found".to_string())?;
    if !o.status.success() {
        return Err("Can't reach the network".into());
    }
    serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())
}

fn fetch_weather() -> Result<Wx, String> {
    let loc = curl("http://ip-api.com/json/?fields=status,city,lat,lon")?;
    let (lat, lon) = (
        loc["lat"].as_f64().ok_or("no location")?,
        loc["lon"].as_f64().ok_or("no location")?,
    );
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1"
    );
    let w = curl(&url)?;
    let cur = &w["current"];
    Ok(Wx {
        city: loc["city"].as_str().unwrap_or("Here").to_string(),
        temp: cur["temperature_2m"].as_f64().ok_or("bad data")?,
        code: cur["weather_code"].as_i64().unwrap_or(0),
        wind: cur["wind_speed_10m"].as_f64().unwrap_or(0.0),
        hum: cur["relative_humidity_2m"].as_f64().unwrap_or(0.0),
        hi: w["daily"]["temperature_2m_max"][0].as_f64().unwrap_or(0.0),
        lo: w["daily"]["temperature_2m_min"][0].as_f64().unwrap_or(0.0),
    })
}

fn wx_text(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 | 2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61..=67 | 80..=82 => "Rain",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "Unsettled",
    }
}

fn wx_icon(p: &Painter, c: Pos2, s: f32, code: i64) {
    let sun = |at: Pos2, r: f32| {
        p.circle_filled(at, r, Color32::from_rgb(255, 205, 80));
        for i in 0..8 {
            let a = i as f32 / 8.0 * TAU;
            let d = vec2(a.cos(), a.sin());
            p.line_segment(
                [at + d * r * 1.35, at + d * r * 1.7],
                Stroke::new(2.0, Color32::from_rgb(255, 205, 80)),
            );
        }
    };
    let cloud = |at: Pos2, k: f32| {
        let col = Color32::WHITE;

        p.circle_filled(at + vec2(-k * 0.5, 0.0), k * 0.45, col);
        p.circle_filled(at + vec2(0.0, -k * 0.25), k * 0.55, col);
        p.circle_filled(at + vec2(k * 0.55, 0.0), k * 0.45, col);

        p.rect_filled(
            Rect::from_min_max(
                at + vec2(-k * 0.5, 0.0),
                at + vec2(k * 0.55, 0.45 * k)
            ),
            0.0,
            col,
    );
    };
    match code {
        0 => sun(c, s * 0.45),
        1 | 2 => {
            sun(c + vec2(s * 0.2, -s * 0.2), s * 0.3);
            cloud(c + vec2(-s * 0.05, s * 0.15), s * 0.7);
        }
        _ => {
            cloud(c + vec2(0.0, -s * 0.1), s * 0.8);
            let col = if matches!(code, 71..=77 | 85 | 86) {
                Color32::WHITE
            } else {
                Color32::from_rgb(110, 170, 255)
            };
            if code >= 51 {
                for i in 0..3 {
                    let x = c.x - s * 0.3 + i as f32 * s * 0.3;
                    p.line_segment(
                        [pos2(x, c.y + s * 0.45), pos2(x - 3.0, c.y + s * 0.7)],
                        Stroke::new(2.0, col),
                    );
                }
            }
        }
    }
}

pub struct Weather {
    state: WState,
}

impl Weather {
    pub fn new() -> Self {
        Self {
            state: WState::Idle,
        }
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_weather());
            ctx.request_repaint();
        });
        self.state = WState::Loading(rx);
    }
}

impl Applet for Weather {
    fn name(&self) -> &'static str {
        "Weather"
    }
    fn kind(&self) -> Kind {
        Kind::Weather
    }
    fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let stale = match &self.state {
            WState::Idle => true,
            WState::Ready(_, at) => at.elapsed() > Duration::from_secs(900),
            WState::Failed(_, at) => at.elapsed() > Duration::from_secs(60),
            WState::Loading(_) => false,
        };
        if stale {
            self.refresh(&ctx);
        }
        if let WState::Loading(rx) = &self.state {
            match rx.try_recv() {
                Ok(Ok(w)) => self.state = WState::Ready(w, Instant::now()),
                Ok(Err(e)) => self.state = WState::Failed(e, Instant::now()),
                Err(_) => ctx.request_repaint_after(Duration::from_millis(300)),
            }
        }
        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.scope_id().with("wx"), Sense::click());
        if resp.clicked() && !matches!(self.state, WState::Loading(_)) {
            self.refresh(&ctx);
        }
        let p = ui.painter().clone();
        match &self.state {
            WState::Ready(w, _) => {
                p.text(
                    rect.left_top() + vec2(0.0, 6.0),
                    Align2::LEFT_CENTER,
                    &w.city,
                    FontId::proportional(12.5),
                    dim(190),
                );
                p.text(
                    rect.left_top() + vec2(0.0, 44.0),
                    Align2::LEFT_CENTER,
                    format!("{:.0}°", w.temp),
                    bold(46.0),
                    Color32::WHITE,
                );
                p.text(
                    rect.left_top() + vec2(0.0, 82.0),
                    Align2::LEFT_CENTER,
                    wx_text(w.code),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                p.text(
                    rect.left_top() + vec2(0.0, 100.0),
                    Align2::LEFT_CENTER,
                    format!("H {:.0}°  L {:.0}°", w.hi, w.lo),
                    FontId::proportional(12.0),
                    dim(170),
                );
                p.text(
                    rect.left_bottom() - vec2(0.0, 2.0),
                    Align2::LEFT_BOTTOM,
                    format!("Humidity {:.0}%  ·  Wind {:.0} km/h", w.hum, w.wind),
                    FontId::proportional(11.0),
                    dim(150),
                );
                wx_icon(
                    &p,
                    pos2(rect.right() - 38.0, rect.top() + 40.0),
                    56.0,
                    w.code,
                );
            }
            WState::Failed(e, _) => {
                p.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    format!("{e}\nclick to retry"),
                    FontId::proportional(12.5),
                    dim(170),
                );
            }
            _ => {
                p.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Fetching weather…",
                    FontId::proportional(12.5),
                    dim(170),
                );
            }
        }
    }
}

const VW: usize = 640;
const VH: usize = 360;
const MEDIA: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "m4a", "opus", "mp4", "mkv", "webm", "mov", "avi",
];

struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Player {
    path: Option<PathBuf>,
    dur: f64,
    base: f64,
    started: Instant,
    playing: bool,
    has_video: bool,
    video: Option<(Kill, Receiver<Vec<u8>>)>,
    audio: Option<Kill>,
    tex: Option<TextureHandle>,
    picker: Picker,
    err: Option<String>,
}

impl Player {
    pub fn new() -> Self {
        Self {
            path: None,
            dur: 0.0,
            base: 0.0,
            started: Instant::now(),
            playing: false,
            has_video: false,
            video: None,
            audio: None,
            tex: None,
            picker: Picker::default(),
            err: None,
        }
    }

    fn load(&mut self, path: &Path) {
        self.stop();
        let probe = |args: &[&str]| {
            Command::new("ffprobe")
                .args(["-v", "error"])
                .args(args)
                .arg(path)
                .stderr(Stdio::null())
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        };
        let Some(d) = probe(&["-show_entries", "format=duration", "-of", "csv=p=0"]) else {
            self.err = Some("ffprobe was not found on PATH".into());
            return;
        };
        self.dur = d.parse().unwrap_or(0.0);
        self.has_video = probe(&[
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .is_some_and(|s| !s.is_empty());
        self.path = Some(path.to_path_buf());
        self.base = 0.0;
        self.tex = None;
        self.err = None;
    }

    fn pos(&self) -> f64 {
        let p = if self.playing {
            self.base + self.started.elapsed().as_secs_f64()
        } else {
            self.base
        };
        if self.dur > 0.0 {
            p.min(self.dur)
        } else {
            p
        }
    }

    fn stop(&mut self) {
        self.video = None;
        self.audio = None;
    }

    fn start(&mut self, at: f64) {
        self.stop();
        let Some(path) = self.path.clone() else {
            return;
        };
        let ss = format!("{at:.2}");
        self.audio = Command::new("ffplay")
            .args(["-v", "quiet", "-nodisp", "-autoexit", "-vn", "-ss", &ss])
            .arg(&path)
            .stdin(Stdio::null())
            .spawn()
            .map(Kill)
            .map_err(|_| self.err = Some("ffplay was not found on PATH".into()))
            .ok();
        if self.has_video {
            let filter = format!("scale={VW}:{VH}:force_original_aspect_ratio=decrease,pad={VW}:{VH}:(ow-iw)/2:(oh-ih)/2,fps=24");
            if let Ok(mut child) = Command::new("ffmpeg")
                .args(["-v", "error", "-ss", &ss, "-re", "-i"])
                .arg(&path)
                .args([
                    "-an", "-sn", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut out) = child.stdout.take() {
                    let (tx, rx) = sync_channel(2);
                    std::thread::spawn(move || loop {
                        let mut buf = vec![0u8; VW * VH * 4];
                        if out.read_exact(&mut buf).is_err() || tx.send(buf).is_err() {
                            break;
                        }
                    });
                    self.video = Some((Kill(child), rx));
                }
            }
        }
        self.base = at;
        self.started = Instant::now();
        self.playing = true;
    }

    fn pause(&mut self) {
        self.base = self.pos();
        self.playing = false;
        self.stop();
    }
}

impl Applet for Player {
    fn name(&self) -> &'static str {
        "Player"
    }
    fn kind(&self) -> Kind {
        Kind::Player
    }
    fn titled(&self) -> bool {
        false
    }
    fn save(&mut self, _d: &Path) -> Option<Value> {
        self.path
            .as_ref()
            .map(|p| json!({ "path": p.to_string_lossy() }))
    }
    fn restore(&mut self, _c: &egui::Context, _d: &Path, data: &Value) {
        if let Some(p) = data.get("path").and_then(|v| v.as_str()) {
            if Path::new(p).exists() {
                self.load(Path::new(p));
            }
        }
    }
    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some(p) = self.picker.poll(&ctx) {
            self.load(&p);
            self.start(0.0);
        }
        let rect = ui.max_rect();
        if self.path.is_none() {
            let resp = ui.interact(rect, ui.scope_id().with("pl_open"), Sense::click());
            ui.painter()
                .rect_filled(rect, 24.0, dim(if resp.hovered() { 26 } else { 16 }));
            let msg = self.err.as_deref().unwrap_or("Open audio or video");
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(13.0),
                dim(170),
            );
            if resp.clicked() {
                self.picker.open(&ctx, "Open media", MEDIA);
            }
            return;
        }

        let pos = self.pos();
        if self.playing && self.dur > 0.0 && pos >= self.dur - 0.05 {
            self.pause();
            self.base = 0.0;
        }

        let mut latest = None;
        if let Some((_, rx)) = &self.video {
            while let Ok(f) = rx.try_recv() {
                latest = Some(f);
            }
        }
        if let Some(f) = latest {
            let img = ColorImage::from_rgba_unmultiplied([VW, VH], &f);
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => {
                    self.tex = Some(ctx.load_texture("player_frame", img, TextureOptions::LINEAR))
                }
            }
        }

        let ctl_h = 30.0;
        let bar = Rect::from_min_size(
            pos2(rect.min.x, rect.max.y - ctl_h - 14.0),
            vec2(rect.width(), 8.0),
        );
        let ctl = Rect::from_min_max(pos2(rect.min.x, rect.max.y - ctl_h), rect.max);
        let stage = Rect::from_min_max(rect.min, pos2(rect.max.x, bar.min.y - 8.0));

        if self.has_video && self.tex.is_some() {
            let tex = self.tex.as_ref().unwrap();
            let fit = (stage.width() / VW as f32).min(stage.height() / VH as f32);
            let r = Rect::from_center_size(stage.center(), vec2(VW as f32, VH as f32) * fit);
            egui::Image::new((tex.id(), vec2(VW as f32, VH as f32)))
                .corner_radius(14.0)
                .paint_at(ui, r);
        } else {
            ui.painter().rect_filled(stage, 18.0, dim(16));
            let name = self
                .path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map_or(String::new(), |n| n.to_string_lossy().into_owned());
            ui.painter().text(
                stage.center() - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                "♪",
                bold(34.0),
                dim(200),
            );
            ui.painter().text(
                stage.center() + vec2(0.0, 22.0),
                Align2::CENTER_CENTER,
                name,
                FontId::proportional(12.5),
                dim(190),
            );
        }

        let frac = if self.dur > 0.0 {
            (pos / self.dur) as f32
        } else {
            0.0
        };
        let hit = ui.interact(
            bar.expand2(vec2(0.0, 6.0)),
            ui.scope_id().with("pl_bar"),
            Sense::click(),
        );
        ui.painter().rect_filled(bar, 4.0, dim(36));
        ui.painter().rect_filled(
            Rect::from_min_size(bar.min, vec2(bar.width() * frac, bar.height())),
            4.0,
            ACCENT,
        );
        if hit.clicked() && self.dur > 0.0 {
            if let Some(pt) = hit.interact_pointer_pos() {
                let at = ((pt.x - bar.min.x) / bar.width()).clamp(0.0, 1.0) as f64 * self.dur;
                self.start(at);
            }
        }

        let w = 44.0;
        let mut x = ctl.center().x - w * 2.0 - 6.0;
        let mut btn = |ui: &mut Ui, label: &str| {
            let r = Rect::from_min_size(pos2(x, ctl.min.y + 2.0), vec2(w, 26.0));
            x += w + 4.0;
            pill(ui, r, label, false)
        };
        if btn(ui, "−10") {
            let at = (self.pos() - 10.0).max(0.0);
            if self.playing {
                self.start(at)
            } else {
                self.base = at
            }
        }
        let label = if self.playing { "Pause" } else { "Play" };
        if btn(ui, label) {
            if self.playing {
                self.pause()
            } else {
                let at = self.pos();
                self.start(at)
            }
        }
        if btn(ui, "+10") {
            let at = (self.pos() + 10.0).min(self.dur.max(0.0));
            if self.playing {
                self.start(at)
            } else {
                self.base = at
            }
        }
        if btn(ui, "Open") {
            self.picker.open(&ctx, "Open media", MEDIA);
        }
        ui.painter().text(
            pos2(ctl.min.x, ctl.center().y),
            Align2::LEFT_CENTER,
            format!("{} / {}", mmss(pos), mmss(self.dur)),
            FontId::proportional(11.0),
            dim(160),
        );

        if self.playing {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

pub struct Calculator {
    cur: String,
    acc: Option<f64>,
    op: Option<char>,
    fresh: bool,
}

fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "Error".into();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{:.8}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

impl Calculator {
    pub fn new() -> Self {
        Self {
            cur: "0".into(),
            acc: None,
            op: None,
            fresh: false,
        }
    }
    fn apply(&mut self) {
        let v = self.cur.parse::<f64>().unwrap_or(0.0);
        let r = match (self.acc, self.op) {
            (Some(a), Some('+')) => a + v,
            (Some(a), Some('−')) => a - v,
            (Some(a), Some('×')) => a * v,
            (Some(a), Some('÷')) => a / v,
            _ => v,
        };
        self.acc = Some(r);
        self.cur = fmt_num(r);
    }
    fn press(&mut self, k: &str) {
        match k {
            "C" => *self = Self::new(),
            "±" => {
                if self.cur.starts_with('-') {
                    self.cur.remove(0);
                } else if self.cur != "0" {
                    self.cur.insert(0, '-');
                }
            }
            "%" => self.cur = fmt_num(self.cur.parse::<f64>().unwrap_or(0.0) / 100.0),
            "." => {
                if self.fresh {
                    self.cur = "0".into();
                    self.fresh = false;
                }
                if !self.cur.contains('.') {
                    self.cur.push('.');
                }
            }
            "+" | "−" | "×" | "÷" => {
                self.apply();
                self.op = k.chars().next();
                self.fresh = true;
            }
            "=" => {
                self.apply();
                self.op = None;
                self.fresh = true;
            }
            d => {
                if self.fresh || self.cur == "0" || self.cur == "Error" {
                    self.cur = d.into();
                    self.fresh = false;
                } else if self.cur.len() < 14 {
                    self.cur.push_str(d);
                }
            }
        }
    }
}

impl Applet for Calculator {
    fn name(&self) -> &'static str {
        "Calculator"
    }
    fn kind(&self) -> Kind {
        Kind::Calculator
    }
    fn ui(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let disp_h = 38.0;
        ui.painter().text(
            pos2(rect.right(), rect.min.y + disp_h / 2.0),
            Align2::RIGHT_CENTER,
            &self.cur,
            bold((disp_h * 0.8).min(30.0)),
            Color32::WHITE,
        );
        let keys = [
            ["C", "±", "%", "÷"],
            ["7", "8", "9", "×"],
            ["4", "5", "6", "−"],
            ["1", "2", "3", "+"],
            ["0", ".", "=", "="],
        ];
        let (gap, top) = (5.0, rect.min.y + disp_h + 4.0);
        let w = (rect.width() - gap * 3.0) / 4.0;
        let h = ((rect.max.y - top - gap * 4.0) / 5.0).max(14.0);
        let mut pressed = None;
        for (r, row) in keys.iter().enumerate() {
            for (c, k) in row.iter().enumerate() {
                if r == 4 && c == 3 {
                    continue;
                }
                let span = if r == 4 && c == 2 { 1.0 } else { 1.0 };
                let b = Rect::from_min_size(
                    pos2(
                        rect.min.x + c as f32 * (w + gap),
                        top + r as f32 * (h + gap),
                    ),
                    vec2(w * span, h),
                );
                let op = matches!(*k, "÷" | "×" | "−" | "+" | "=");
                let resp = ui.interact(b, ui.scope_id().with(("calc", r, c)), Sense::click());
                let base = if op {
                    ACCENT.gamma_multiply(if resp.hovered() { 1.0 } else { 0.8 })
                } else {
                    dim(if resp.hovered() { 44 } else { 24 })
                };
                ui.painter().rect_filled(b, h.min(w) / 2.0, base);
                ui.painter().text(
                    b.center(),
                    Align2::CENTER_CENTER,
                    k,
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
                if resp.clicked() {
                    pressed = Some(*k);
                }
            }
        }
        if let Some(k) = pressed {
            self.press(k);
        }
    }
}

pub struct TimerApp {
    timer: bool,
    run: bool,
    base: f64,
    started: Instant,
    target: f64,
}

impl TimerApp {
    pub fn new() -> Self {
        Self {
            timer: false,
            run: false,
            base: 0.0,
            started: Instant::now(),
            target: 300.0,
        }
    }
    fn elapsed(&self) -> f64 {
        self.base
            + if self.run {
                self.started.elapsed().as_secs_f64()
            } else {
                0.0
            }
    }
}

impl Applet for TimerApp {
    fn name(&self) -> &'static str {
        "Timer"
    }
    fn kind(&self) -> Kind {
        Kind::Timer
    }
    fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.timer, "Stopwatch").clicked() {
                self.timer = false;
                self.run = false;
                self.base = 0.0;
            }
            if ui.selectable_label(self.timer, "Timer").clicked() {
                self.timer = true;
                self.run = false;
                self.base = 0.0;
            }
            if self.timer {
                for m in [1, 5, 10, 25] {
                    if ui.small_button(format!("{m}m")).clicked() {
                        self.target = m as f64 * 60.0;
                        self.run = false;
                        self.base = 0.0;
                    }
                }
            }
        });
        let el = self.elapsed();
        if self.timer && self.run && el >= self.target {
            self.run = false;
            self.base = self.target;
        }
        let rect = ui.max_rect();
        let area = Rect::from_min_max(
            pos2(rect.min.x, ui.cursor().min.y + 2.0),
            pos2(rect.max.x, rect.max.y - 30.0),
        );
        let d = area.width().min(area.height());
        let (c, w) = (area.center(), (d * 0.07).clamp(5.0, 14.0));
        let (frac, label) = if self.timer {
            ((el / self.target) as f32, mmss((self.target - el).ceil()))
        } else {
            (
                ((el % 60.0) / 60.0) as f32,
                format!("{}.{}", mmss(el), (el.fract() * 10.0) as u32),
            )
        };
        ring(ui.painter(), c, d / 2.0 - w, w, frac, TAU);
        ui.painter().text(
            c,
            Align2::CENTER_CENTER,
            label,
            bold((d * 0.2).clamp(14.0, 40.0)),
            Color32::WHITE,
        );
        ui.allocate_rect(area, Sense::hover());
        ui.horizontal(|ui| {
            if ui
                .button(if self.run { "Pause" } else { "Start" })
                .clicked()
            {
                if self.run {
                    self.base = self.elapsed();
                    self.run = false;
                } else {
                    self.started = Instant::now();
                    self.run = true;
                }
            }
            if ui.button("Reset").clicked() {
                self.run = false;
                self.base = 0.0;
            }
        });
        if self.run {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}

pub struct Habits {
    items: Vec<(String, Vec<i64>)>,
    input: String,
}

impl Habits {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            input: String::new(),
        }
    }
}

impl Applet for Habits {
    fn name(&self) -> &'static str {
        "Habits"
    }
    fn kind(&self) -> Kind {
        Kind::Habits
    }
    fn save(&mut self, _d: &Path) -> Option<Value> {
        Some(json!({ "items": self.items }))
    }
    fn restore(&mut self, _c: &egui::Context, _d: &Path, data: &Value) {
        if let Some(v) = data
            .get("items")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
        {
            self.items = v;
        }
    }
    fn ui(&mut self, ui: &mut Ui) {
        let today = now().day;
        let f = ui.add(
            egui::TextEdit::singleline(&mut self.input)
                .hint_text("New habit")
                .desired_width(f32::INFINITY),
        );
        if f.lost_focus()
            && ui.input(|i| i.key_pressed(Key::Enter))
            && !self.input.trim().is_empty()
        {
            self.items.push((self.input.trim().to_string(), Vec::new()));
            self.input.clear();
            f.request_focus();
        }
        let mut del = None;
        egui::ScrollArea::vertical()
            .id_salt("habits")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, (name, done)) in self.items.iter_mut().enumerate() {
                    let mut streak = 0;
                    while done.contains(&(today - streak)) {
                        streak += 1;
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name.as_str()).font(bold(12.5)));
                        ui.label(RichText::new(format!("{streak}d streak")).weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Label::new(RichText::new("×").weak())
                                        .sense(Sense::click()),
                                )
                                .clicked()
                            {
                                del = Some(i);
                            }
                            for back in 0..7 {
                                let day = today - back;
                                let (r, resp) =
                                    ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                                let on = done.contains(&day);
                                if on {
                                    ui.painter().circle_filled(r.center(), 6.5, ACCENT);
                                } else {
                                    ui.painter().circle_stroke(
                                        r.center(),
                                        6.5,
                                        Stroke::new(1.2, dim(if back == 0 { 170 } else { 80 })),
                                    );
                                }
                                if resp.clicked() {
                                    if on {
                                        done.retain(|d| *d != day);
                                    } else {
                                        done.push(day);
                                    }
                                }
                            }
                        });
                    });
                }
            });
        if let Some(i) = del {
            self.items.remove(i);
        }
    }
}

pub struct Countdown {
    name: String,
    date: String,
}

impl Countdown {
    pub fn new() -> Self {
        let t = now();
        Self {
            name: "New Year".into(),
            date: format!("{}-01-01", t.y + 1),
        }
    }
}

impl Applet for Countdown {
    fn name(&self) -> &'static str {
        "Countdown"
    }
    fn kind(&self) -> Kind {
        Kind::Countdown
    }
    fn save(&mut self, _d: &Path) -> Option<Value> {
        Some(json!({ "name": self.name, "date": self.date }))
    }
    fn restore(&mut self, _c: &egui::Context, _d: &Path, data: &Value) {
        if let Some(s) = data.get("name").and_then(|v| v.as_str()) {
            self.name = s.into();
        }
        if let Some(s) = data.get("date").and_then(|v| v.as_str()) {
            self.date = s.into();
        }
    }
    fn ui(&mut self, ui: &mut Ui) {
        let parts: Vec<i64> = self
            .date
            .split('-')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        let left =
            (parts.len() == 3 && (1..=12).contains(&parts[1]) && (1..=31).contains(&parts[2]))
                .then(|| days_from_civil(parts[0], parts[1], parts[2]) - now().day);
        let rect = ui.max_rect();
        let c = rect.center() - vec2(0.0, 14.0);
        let (big, small) = match left {
            Some(d) if d > 0 => (
                d.to_string(),
                if d == 1 { "day to go" } else { "days to go" },
            ),
            Some(0) => ("Today".into(), "is the day"),
            Some(_) => ((-left.unwrap()).to_string(), "days ago"),
            None => ("?".into(), "use YYYY-MM-DD"),
        };
        ui.painter().text(
            c,
            Align2::CENTER_CENTER,
            big,
            bold((rect.height() * 0.3).clamp(24.0, 64.0)),
            Color32::WHITE,
        );
        ui.painter().text(
            c + vec2(0.0, rect.height() * 0.2 + 8.0),
            Align2::CENTER_CENTER,
            format!("{small} · {}", self.name),
            FontId::proportional(12.5),
            dim(190),
        );
        let row = Rect::from_min_max(pos2(rect.min.x, rect.max.y - 26.0), rect.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(row.width() * 0.5));
                ui.add(egui::TextEdit::singleline(&mut self.date).desired_width(f32::INFINITY));
            });
        });
    }
}

pub struct WorldClock;

impl WorldClock {
    pub fn new() -> Self {
        Self
    }
}

impl Applet for WorldClock {
    fn name(&self) -> &'static str {
        "World clock"
    }
    fn kind(&self) -> Kind {
        Kind::WorldClock
    }
    fn ui(&mut self, ui: &mut Ui) {
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        let here = local_offset();
        let zones: [(&str, i64); 6] = [
            ("New York", -5 * 3600),
            ("London", 0),
            ("Dubai", 4 * 3600),
            ("Kolkata", 19_800),
            ("Tokyo", 9 * 3600),
            ("Sydney", 11 * 3600),
        ];
        let rect = ui.max_rect();
        let row_h = (rect.height() / zones.len() as f32).clamp(18.0, 34.0);
        let p = ui.painter().clone();
        for (i, (name, off)) in zones.iter().enumerate() {
            let t = now_at(*off);
            let y = rect.min.y + (i as f32 + 0.5) * row_h;
            let rel = (t.day - now_at(here).day) as i64;
            let tag = match rel {
                1 => "Tomorrow",
                -1 => "Yesterday",
                _ => "Today",
            };
            p.text(
                pos2(rect.min.x, y),
                Align2::LEFT_CENTER,
                *name,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            p.text(
                pos2(rect.min.x + rect.width() * 0.48, y),
                Align2::LEFT_CENTER,
                tag,
                FontId::proportional(11.0),
                dim(130),
            );
            p.text(
                pos2(rect.max.x, y),
                Align2::RIGHT_CENTER,
                format!("{:02}:{:02}", t.h, t.mi),
                bold(15.0),
                Color32::WHITE,
            );
        }
        let _ = Shape::Noop;
    }
}
