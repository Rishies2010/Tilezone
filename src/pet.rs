use eframe::egui::{pos2, vec2, Color32, Context, Painter, Pos2, Rect, Shape, TextureHandle, TextureOptions, Vec2};

use crate::sprites;

const CELL: usize = 32;
const IDLE: [usize; 2] = [0, 1];
const CLEAN: [usize; 2] = [2, 3];
const WALK: usize = 4;
const RUN: usize = 5;
const SLEEP: usize = 6;
const PAW: usize = 7;
const JUMP: usize = 8;
const SCARED: usize = 9;

const LEAP_TIME: f32 = 0.7;
const REACH: f32 = 210.0;

#[derive(Clone, Copy)]
pub struct Platform {
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

pub struct Input {
    pub cursor: Option<Pos2>,
    pub click: Option<Pos2>,
    pub down: bool,
    pub active: bool,
    pub follow: bool,
    pub interval: f32,
    pub scale: f32,
}

pub struct Sheet {
    tex: TextureHandle,
    size: [usize; 2],
    frames: Vec<usize>,
}

impl Sheet {
    pub fn load(ctx: &Context) -> Self {
        let (size, raw) = sprites::decode(sprites::PET);
        let rows = size[1] / CELL;
        let cols = size[0] / CELL;
        let mut frames = vec![0; rows];
        for (r, count) in frames.iter_mut().enumerate() {
            for c in 0..cols {
                let filled = (0..CELL).any(|y| {
                    (0..CELL).any(|x| raw[((r * CELL + y) * size[0] + c * CELL + x) * 4 + 3] > 0)
                });
                if filled {
                    *count = c + 1;
                }
            }
        }
        let image = eframe::egui::ColorImage::from_rgba_unmultiplied(size, &raw);
        Self { tex: ctx.load_texture("pet_sheet", image, TextureOptions::NEAREST), size, frames }
    }

    fn count(&self, row: usize) -> usize {
        self.frames.get(row).copied().unwrap_or(1).max(1)
    }

    fn draw(&self, p: &Painter, row: usize, frame: usize, feet: Pos2, scale: f32, flip: bool) {
        let row = row.min(self.frames.len().saturating_sub(1));
        let frame = frame.min(self.count(row) - 1);
        let (w, h) = (self.size[0] as f32, self.size[1] as f32);
        let (mut u0, mut u1) = ((frame * CELL) as f32 / w, ((frame + 1) * CELL) as f32 / w);
        if flip {
            std::mem::swap(&mut u0, &mut u1);
        }
        let (v0, v1) = ((row * CELL) as f32 / h, ((row + 1) * CELL) as f32 / h);
        let size = CELL as f32 * scale;
        let rect = Rect::from_min_size(pos2(feet.x - size / 2.0, feet.y - size), vec2(size, size));
        p.image(self.tex.id(), rect, Rect::from_min_max(pos2(u0, v0), pos2(u1, v1)), Color32::WHITE);
    }
}

#[derive(Clone, Copy)]
enum State {
    Sit { left: f32, row: usize },
    Walk,
    Run,
    Sleep,
    Hop { from: Pos2, to: Pos2, dur: f32, lift: f32, target: Option<u64> },
    Fall { vy: f32, vx: f32 },
    Touch { anchor: Pos2, left: f32 },
    Scared { left: f32 },
    Leap { left: f32 },
    Held { grab: Vec2, origin: Pos2, moved: bool },
}

pub struct Pet {
    pos: Pos2,
    on: Option<u64>,
    dir: f32,
    state: State,
    t: f32,
    timer: f32,
    idle: f32,
    next_chase: f32,
    chase: Option<f32>,
    flee: bool,
    vel: Vec2,
    rng: u64,
}

fn seg_dist(p: &Platform, c: Pos2) -> f32 {
    (c - pos2(c.x.clamp(p.x0, p.x1), p.y)).length()
}

impl Pet {
    pub fn new() -> Self {
        Self {
            pos: pos2(-300.0, -300.0),
            on: None,
            dir: 1.0,
            state: State::Fall { vy: 0.0, vx: 0.0 },
            t: 0.0,
            timer: 2.0,
            idle: 0.0,
            next_chase: 15.0,
            chase: None,
            flee: false,
            vel: Vec2::ZERO,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    pub fn repaint_after(&self, reduce: bool) -> std::time::Duration {
        let ms = match self.state {
            State::Sleep => 250,
            _ if reduce => 33,
            _ => 16,
        };
        std::time::Duration::from_millis(ms)
    }

    pub fn held(&self) -> bool {
        matches!(self.state, State::Held { .. })
    }

    pub fn hit(&self, c: Pos2, scale: f32) -> bool {
        Rect::from_center_size(pos2(self.pos.x, self.pos.y - 16.0 * scale), vec2(22.0, 26.0) * scale).contains(c)
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn set(&mut self, s: State) {
        self.state = s;
        self.t = 0.0;
    }

    fn end_chase(&mut self, interval: f32) {
        self.chase = None;
        self.next_chase = interval * (0.7 + 0.6 * self.rand());
    }

    fn reachable(&self, plats: &[Platform]) -> Vec<Platform> {
        plats
            .iter()
            .copied()
            .filter(|p| Some(p.id) != self.on)
            .filter(|p| {
                let gap = (p.x0 - self.pos.x).max(self.pos.x - p.x1).max(0.0);
                gap < REACH && (-170.0..420.0).contains(&(p.y - self.pos.y))
            })
            .collect()
    }

    fn hop_to(&mut self, p: Platform) {
        let to = pos2(self.pos.x.clamp(p.x0, p.x1), p.y);
        if to.x != self.pos.x {
            self.dir = (to.x - self.pos.x).signum();
        }
        let dist = (to - self.pos).length();
        let lift = 36.0 + (self.pos.y - to.y).max(0.0);
        self.set(State::Hop { from: self.pos, to, dur: 0.38 + dist / 1100.0, lift, target: Some(p.id) });
    }

    fn wander(&mut self, p: Platform, plats: &[Platform]) {
        self.timer = 1.0 + self.rand() * 3.0;
        let roll = self.rand();
        if roll < 0.35 {
            let options = self.reachable(plats);
            if !options.is_empty() {
                let i = (self.rand() * options.len() as f32) as usize % options.len();
                self.hop_to(options[i]);
                return;
            }
        }
        if roll < 0.65 {
            let row = if self.rand() < 0.7 { IDLE[(self.rand() * 2.0) as usize % 2] } else { CLEAN[(self.rand() * 2.0) as usize % 2] };
            let left = 1.5 + self.rand() * 3.0;
            self.set(State::Sit { left, row });
            return;
        }
        self.dir = if self.pos.x <= p.x0 + 1.0 {
            1.0
        } else if self.pos.x >= p.x1 - 1.0 {
            -1.0
        } else if self.rand() < 0.5 {
            -1.0
        } else {
            1.0
        };
        self.set(State::Walk);
    }

    fn chase_step(&mut self, c: Pos2, p: Platform, plats: &[Platform], inp: &Input) {
        let dist = (c - self.pos).length();
        if dist < REACH {
            let to = pos2(c.x, c.y + 22.0 * inp.scale / 2.0);
            self.dir = if to.x >= self.pos.x { 1.0 } else { -1.0 };
            self.set(State::Hop { from: self.pos, to, dur: 0.35 + dist / 1100.0, lift: 46.0, target: None });
            return;
        }
        let here = seg_dist(&p, c);
        let better = self
            .reachable(plats)
            .into_iter()
            .filter(|q| seg_dist(q, c) < here - 10.0)
            .min_by(|a, b| seg_dist(a, c).total_cmp(&seg_dist(b, c)));
        if let Some(q) = better {
            self.hop_to(q);
            return;
        }
        let dx = c.x - self.pos.x;
        let blocked = (self.pos.x <= p.x0 + 0.5 && dx < 0.0) || (self.pos.x >= p.x1 - 0.5 && dx > 0.0);
        if dx.abs() > 24.0 && !blocked {
            self.dir = dx.signum();
            if !matches!(self.state, State::Run) {
                self.set(State::Run);
            }
            self.timer = 2.0;
            return;
        }
        self.end_chase(inp.interval);
        self.set(State::Sit { left: 1.0, row: IDLE[0] });
    }

    pub fn update(&mut self, dt: f32, plats: &[Platform], bounds: Rect, inp: &Input) {
        self.t += dt;
        self.idle = if inp.active { 0.0 } else { self.idle + dt };

        if self.pos.x < -100.0 && !plats.is_empty() {
            self.pos = pos2(bounds.center().x, bounds.min.y - 40.0);
        }

        if let Some(c) = inp.click {
            if !matches!(self.state, State::Held { .. }) && self.hit(c, inp.scale) {
                self.end_chase(inp.interval);
                self.flee = false;
                self.vel = Vec2::ZERO;
                self.set(State::Held { grab: self.pos - c, origin: c, moved: false });
            }
        }

        let ground = matches!(self.state, State::Sit { .. } | State::Walk | State::Run);
        if !inp.follow || inp.cursor.is_none() {
            if self.chase.is_some() && ground {
                self.end_chase(inp.interval);
            }
        } else if ground {
            if self.chase.is_none() && self.idle < 6.0 {
                self.next_chase -= dt;
                if self.next_chase <= 0.0 {
                    self.chase = Some(14.0);
                }
            }
            if let Some(left) = &mut self.chase {
                *left -= dt;
                if *left <= 0.0 {
                    self.end_chase(inp.interval);
                }
            }
        }

        match self.state {
            State::Held { grab, origin, moved } => {
                if !inp.down {
                    if moved {
                        self.on = None;
                        let (vx, vy) = (self.vel.x.clamp(-600.0, 600.0), self.vel.y.clamp(-900.0, 600.0));
                        self.set(State::Fall { vy, vx });
                    } else if self.on.is_some() {
                        self.set(State::Leap { left: LEAP_TIME });
                    } else {
                        self.set(State::Fall { vy: 0.0, vx: 0.0 });
                    }
                    return;
                }
                if let Some(c) = inp.cursor {
                    let moved = moved || c.distance(origin) > 6.0;
                    if moved {
                        let target = pos2(
                            (c.x + grab.x).clamp(bounds.min.x + 16.0, bounds.max.x - 16.0),
                            (c.y + grab.y).clamp(bounds.min.y + 40.0, bounds.max.y + 20.0),
                        );
                        let v = (target - self.pos) / dt.max(0.001);
                        self.vel = self.vel * 0.6 + v * 0.4;
                        if (target.x - self.pos.x).abs() > 1.0 {
                            self.dir = (target.x - self.pos.x).signum();
                        }
                        self.pos = target;
                        self.on = None;
                    }
                    self.state = State::Held { grab, origin, moved };
                }
            }
            State::Hop { from, to, dur, lift, target } => {
                let mut to = to;
                match target {
                    Some(id) => match plats.iter().find(|p| p.id == id) {
                        Some(p) => to = pos2(to.x.clamp(p.x0, p.x1), p.y),
                        None => {
                            self.set(State::Fall { vy: 0.0, vx: 0.0 });
                            return;
                        }
                    },
                    None => {
                        if let Some(c) = inp.cursor {
                            to = pos2(c.x, c.y + 22.0 * inp.scale / 2.0);
                        }
                    }
                }
                self.state = State::Hop { from, to, dur, lift, target };
                let k = (self.t / dur).min(1.0);
                let e = k * k * (3.0 - 2.0 * k);
                self.pos = from.lerp(to, e) - vec2(0.0, 4.0 * lift * k * (1.0 - k));
                if self.t >= dur {
                    self.pos = to;
                    match target {
                        Some(id) => {
                            self.on = Some(id);
                            self.timer = 0.8 + self.rand() * 2.0;
                            self.set(State::Sit { left: 0.25, row: IDLE[0] });
                        }
                        None => {
                            self.on = None;
                            self.set(State::Touch { anchor: inp.cursor.unwrap_or(to), left: 4.0 });
                        }
                    }
                }
            }
            State::Fall { vy, vx } => {
                let old = self.pos.y;
                let vy = vy + 1800.0 * dt;
                self.pos.y += vy * dt;
                self.pos.x = (self.pos.x + vx * dt).clamp(bounds.min.x + 16.0, bounds.max.x - 16.0);
                self.state = State::Fall { vy, vx };
                let landing = plats
                    .iter()
                    .filter(|p| self.pos.x >= p.x0 - 4.0 && self.pos.x <= p.x1 + 4.0)
                    .filter(|p| old <= p.y + 2.0 && self.pos.y >= p.y)
                    .min_by(|a, b| a.y.total_cmp(&b.y))
                    .copied();
                if let Some(p) = landing {
                    self.pos.y = p.y;
                    self.on = Some(p.id);
                    if self.flee {
                        self.flee = false;
                        self.timer = 1.0;
                        self.set(State::Run);
                    } else {
                        self.set(State::Sit { left: 0.3, row: IDLE[0] });
                    }
                } else if self.pos.y > bounds.max.y + 120.0 && !plats.is_empty() {
                    let p = plats[(self.rand() * plats.len() as f32) as usize % plats.len()];
                    self.pos = pos2(p.x0 + (p.x1 - p.x0) * self.rand(), bounds.min.y - 40.0);
                    self.state = State::Fall { vy: 0.0, vx: 0.0 };
                }
            }
            State::Touch { anchor, left } => {
                let Some(c) = inp.cursor else {
                    self.end_chase(inp.interval);
                    self.set(State::Fall { vy: -120.0, vx: 0.0 });
                    return;
                };
                if self.t < 0.4 {
                    self.state = State::Touch { anchor: c, left };
                } else if c.distance(anchor) > 12.0 {
                    self.dir = if self.pos.x >= c.x { 1.0 } else { -1.0 };
                    self.set(State::Scared { left: 1.1 });
                } else if left - dt <= 0.0 {
                    self.end_chase(inp.interval);
                    self.set(State::Fall { vy: -120.0, vx: 0.0 });
                } else {
                    self.state = State::Touch { anchor, left: left - dt };
                }
            }
            State::Scared { left } => {
                if left - dt <= 0.0 {
                    self.end_chase(inp.interval);
                    self.flee = true;
                    let vx = self.dir * 150.0;
                    self.set(State::Fall { vy: -380.0, vx });
                } else {
                    self.state = State::Scared { left: left - dt };
                }
            }
            State::Leap { left } => {
                if left - dt <= 0.0 {
                    if self.on.is_some() {
                        self.set(State::Sit { left: 0.3, row: IDLE[0] });
                    } else {
                        self.set(State::Fall { vy: 0.0, vx: 0.0 });
                    }
                } else {
                    self.state = State::Leap { left: left - dt };
                }
            }
            State::Sit { .. } | State::Walk | State::Run | State::Sleep => self.ground(dt, plats, inp),
        }
    }

    fn ground(&mut self, dt: f32, plats: &[Platform], inp: &Input) {
        let Some(p) = self.on.and_then(|id| plats.iter().find(|p| p.id == id)).copied() else {
            self.on = None;
            self.set(State::Fall { vy: 0.0, vx: 0.0 });
            return;
        };
        self.pos.y = p.y;
        self.pos.x = self.pos.x.clamp(p.x0, p.x1);
        let chasing = self.chase.is_some();
        let cursor = inp.cursor.filter(|_| chasing);
        let speed = inp.scale / 2.0;

        match self.state {
            State::Sleep => {
                if self.idle < 0.5 {
                    self.set(State::Sit { left: 1.0, row: IDLE[0] });
                }
            }
            State::Sit { left, row } => {
                let left = left - dt;
                if left > 0.0 {
                    self.state = State::Sit { left, row };
                } else if let Some(c) = cursor {
                    self.chase_step(c, p, plats, inp);
                } else if self.idle > 25.0 {
                    self.set(State::Sleep);
                } else {
                    self.wander(p, plats);
                }
            }
            State::Walk => {
                self.pos.x += self.dir * 60.0 * speed * dt;
                self.timer -= dt;
                let at_edge = self.pos.x <= p.x0 || self.pos.x >= p.x1;
                if let Some(c) = cursor {
                    self.chase_step(c, p, plats, inp);
                } else if self.idle > 25.0 {
                    self.set(State::Sit { left: 0.5, row: IDLE[0] });
                } else if at_edge || self.timer <= 0.0 {
                    self.wander(p, plats);
                }
            }
            State::Run => {
                self.pos.x += self.dir * 150.0 * speed * dt;
                self.timer -= dt;
                let at_edge = self.pos.x <= p.x0 || self.pos.x >= p.x1;
                if let Some(c) = cursor {
                    self.chase_step(c, p, plats, inp);
                } else if self.timer <= 0.0 || at_edge {
                    self.set(State::Sit { left: 0.6, row: IDLE[0] });
                }
            }
            _ => {}
        }
    }

    fn frame(&self, sheet: &Sheet) -> (usize, usize, f32, bool) {
        let cyc = |row: usize, fps: f32| (row, (self.t * fps) as usize % sheet.count(row));
        match self.state {
            State::Sit { row, .. } => {
                let (r, f) = cyc(row, 5.0);
                (r, f, 0.0, true)
            }
            State::Walk => {
                let (r, f) = cyc(WALK, 10.0);
                (r, f, 0.0, true)
            }
            State::Run => {
                let (r, f) = cyc(RUN, 14.0);
                (r, f, 0.0, true)
            }
            State::Sleep => {
                let (r, f) = cyc(SLEEP, 2.5);
                (r, f, 0.0, true)
            }
            State::Hop { dur, .. } => {
                let n = sheet.count(JUMP);
                (JUMP, ((self.t / dur) * n as f32) as usize, 0.0, false)
            }
            State::Fall { .. } => (JUMP, 3, 0.0, false),
            State::Touch { .. } => {
                let (r, f) = cyc(PAW, 8.0);
                (r, f, 0.0, false)
            }
            State::Scared { .. } => {
                let (r, f) = cyc(SCARED, 12.0);
                (r, f, 0.0, false)
            }
            State::Held { moved, .. } => {
                if moved {
                    let (r, f) = cyc(SCARED, 12.0);
                    (r, f, 0.0, false)
                } else {
                    let (r, f) = cyc(IDLE[0], 5.0);
                    (r, f, 0.0, true)
                }
            }
            State::Leap { left } => {
                let prog = 1.0 - left / LEAP_TIME;
                let n = sheet.count(JUMP);
                (JUMP, (prog * n as f32) as usize, (prog * std::f32::consts::PI).sin() * 14.0, false)
            }
        }
    }

    pub fn paint(&self, p: &Painter, sheet: &Sheet, scale: f32) {
        let (row, frame, lift, grounded) = self.frame(sheet);
        if grounded || lift > 0.0 {
            let w = 9.0 * scale * (1.0 - lift / 40.0);
            p.add(Shape::ellipse_filled(
                pos2(self.pos.x, self.pos.y + 1.0),
                vec2(w, 2.5 * scale),
                Color32::from_black_alpha(55),
            ));
        }
        let feet = pos2(self.pos.x.round(), (self.pos.y - lift * scale).round());
        sheet.draw(p, row, frame, feet, scale, self.dir < 0.0);
    }
}
