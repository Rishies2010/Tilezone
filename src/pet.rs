use eframe::egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};

const BODY: Color32 = Color32::from_rgb(214, 120, 230);
const DARK: Color32 = Color32::from_rgb(150, 70, 200);
const BELLY: Color32 = Color32::from_rgb(244, 196, 250);

#[derive(Clone, Copy)]
pub struct Platform {
    pub id: u64,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

enum State {
    Walk,
    Sit(f32),
    Sleep,
    Hop { from: Pos2, to: Pos2, t: f32, dur: f32, lift: f32, target: u64 },
    Fall { vy: f32 },
}

pub struct Pet {
    pos: Pos2,
    on: Option<u64>,
    dir: f32,
    state: State,
    timer: f32,
    idle: f32,
    phase: f32,
    squash: f32,
    rng: u64,
}

impl Pet {
    pub fn new() -> Self {
        Self {
            pos: pos2(-200.0, -200.0),
            on: None,
            dir: 1.0,
            state: State::Fall { vy: 0.0 },
            timer: 2.0,
            idle: 0.0,
            phase: 0.0,
            squash: 0.0,
            rng: 0x9E3779B97F4A7C15,
        }
    }

    pub fn animating(&self) -> bool {
        !matches!(self.state, State::Sleep)
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn reachable(&self, plats: &[Platform]) -> Vec<Platform> {
        plats
            .iter()
            .copied()
            .filter(|p| Some(p.id) != self.on)
            .filter(|p| {
                let gap = (p.x0 - self.pos.x).max(self.pos.x - p.x1).max(0.0);
                let dy = p.y - self.pos.y;
                gap < 210.0 && (-170.0..420.0).contains(&dy)
            })
            .collect()
    }

    fn hop_to(&mut self, plats: &[Platform]) -> bool {
        let options = self.reachable(plats);
        if options.is_empty() {
            return false;
        }
        let p = options[(self.rand() * options.len() as f32) as usize % options.len()];
        let tx = self.pos.x.clamp(p.x0, p.x1);
        let to = pos2(tx, p.y);
        self.dir = if to.x == self.pos.x { self.dir } else { (to.x - self.pos.x).signum() };
        let dist = (to - self.pos).length();
        let lift = 36.0 + (self.pos.y - to.y).max(0.0) + 0.1 * (to.y - self.pos.y).max(0.0);
        self.state = State::Hop { from: self.pos, to, t: 0.0, dur: 0.32 + dist / 1100.0, lift, target: p.id };
        true
    }

    pub fn update(&mut self, dt: f32, plats: &[Platform], bounds: Rect, active: bool) {
        self.phase += dt;
        self.squash = (self.squash - dt * 4.0).max(0.0);
        if active {
            self.idle = 0.0;
            if matches!(self.state, State::Sleep) {
                self.state = State::Sit(1.2);
            }
        } else {
            self.idle += dt;
        }

        if self.pos.x < -100.0 && !plats.is_empty() {
            self.pos = pos2(bounds.center().x, bounds.min.y - 30.0);
        }

        let current = self.on.and_then(|id| plats.iter().find(|p| p.id == id)).copied();
        match &mut self.state {
            State::Hop { from, to, t, dur, lift, target } => {
                if let Some(p) = plats.iter().find(|p| p.id == *target) {
                    *to = pos2(to.x.clamp(p.x0, p.x1), p.y);
                } else {
                    self.state = State::Fall { vy: 0.0 };
                    return;
                }
                *t += dt / *dur;
                let k = t.min(1.0);
                let eased = k * k * (3.0 - 2.0 * k);
                self.pos = from.lerp(*to, eased) - vec2(0.0, 4.0 * *lift * k * (1.0 - k));
                if *t >= 1.0 {
                    self.pos = *to;
                    self.on = Some(*target);
                    self.squash = 1.0;
                    self.timer = 0.8 + self.rand() * 2.0;
                    self.state = State::Walk;
                }
            }
            State::Fall { vy } => {
                let old = self.pos.y;
                *vy += 1800.0 * dt;
                self.pos.y += *vy * dt;
                let landing = plats
                    .iter()
                    .filter(|p| self.pos.x >= p.x0 - 4.0 && self.pos.x <= p.x1 + 4.0)
                    .filter(|p| old <= p.y + 2.0 && self.pos.y >= p.y)
                    .min_by(|a, b| a.y.total_cmp(&b.y))
                    .copied();
                if let Some(p) = landing {
                    self.pos.y = p.y;
                    self.on = Some(p.id);
                    self.squash = 1.0;
                    self.state = State::Walk;
                } else if self.pos.y > bounds.max.y + 120.0 && !plats.is_empty() {
                    let p = plats[(self.rand() * plats.len() as f32) as usize % plats.len()];
                    self.pos = pos2(p.x0 + (p.x1 - p.x0) * self.rand(), bounds.min.y - 30.0);
                    self.state = State::Fall { vy: 0.0 };
                }
            }
            State::Walk | State::Sit(_) | State::Sleep => {
                let Some(p) = current else {
                    self.on = None;
                    self.state = State::Fall { vy: 0.0 };
                    return;
                };
                self.pos.y = p.y;
                self.pos.x = self.pos.x.clamp(p.x0, p.x1);

                match &mut self.state {
                    State::Sit(left) => {
                        *left -= dt;
                        if *left <= 0.0 {
                            self.state = if self.idle > 25.0 { State::Sleep } else { State::Walk };
                        }
                    }
                    State::Walk => {
                        self.pos.x += self.dir * 72.0 * dt;
                        self.timer -= dt;
                        let at_edge = self.pos.x <= p.x0 || self.pos.x >= p.x1;
                        if at_edge || self.timer <= 0.0 {
                            self.timer = 1.0 + self.rand() * 3.0;
                            let roll = self.rand();
                            if roll < 0.45 && self.hop_to(plats) {
                            } else if roll < 0.7 {
                                self.state = State::Sit(1.0 + self.rand() * 2.5);
                            } else if at_edge {
                                self.dir = -self.dir;
                                self.pos.x = self.pos.x.clamp(p.x0 + 1.0, p.x1 - 1.0);
                            }
                        }
                        if self.idle > 25.0 {
                            self.state = State::Sit(0.5);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn paint(&self, p: &Painter) {
        let s = self.dir;
        let feet = self.pos;
        let sleeping = matches!(self.state, State::Sleep);
        let sitting = matches!(self.state, State::Sit(_));
        let walking = matches!(self.state, State::Walk);
        let airborne = matches!(self.state, State::Hop { .. } | State::Fall { .. });
        let t = self.phase;

        let bob = if walking { (t * 15.0).sin().abs() * 2.0 } else { 0.0 };
        let squash = self.squash;
        let (bw, bh) = if sleeping {
            (34.0, 15.0)
        } else if sitting {
            (26.0, 24.0)
        } else if airborne {
            (26.0, 22.0)
        } else {
            (30.0 + 4.0 * squash, 20.0 - 5.0 * squash)
        };

        p.add(Shape::ellipse_filled(feet + vec2(0.0, 1.0), vec2(bw * 0.55, 3.0), Color32::from_black_alpha(55)));

        let body_c = feet + vec2(0.0, -bh / 2.0 - 2.0 - bob);
        let tail_base = body_c + vec2(-s * bw / 2.0 + s * 3.0, 2.0);
        let wag = if sleeping { 0.0 } else { (t * 5.0).sin() };
        let tail: Vec<Pos2> = (0..=6)
            .map(|i| {
                let k = i as f32 / 6.0;
                tail_base + vec2(-s * (k * 14.0), -k * 16.0 + (k * 3.0 + t * 4.0).sin() * 2.0 * wag * k)
            })
            .collect();
        p.add(Shape::line(tail, Stroke::new(4.5, DARK)));

        let leg = |x: f32, lift: f32| {
            let c = feet + vec2(x, -3.0 - lift);
            p.rect_filled(Rect::from_center_size(c, vec2(5.0, 7.0)), 2.5, DARK);
        };
        let stride = if walking { (t * 15.0).sin() * 3.0 } else { 0.0 };
        if !sleeping {
            leg(s * (bw * 0.28) + stride, stride.max(0.0));
            leg(-s * (bw * 0.28) - stride, (-stride).max(0.0));
        }

        p.rect_filled(Rect::from_center_size(body_c, vec2(bw, bh)), bh / 2.0, BODY);
        p.rect_filled(Rect::from_center_size(body_c + vec2(0.0, bh * 0.22), vec2(bw * 0.6, bh * 0.4)), bh * 0.2, BELLY);

        let head = if sleeping {
            body_c + vec2(s * (bw / 2.0 - 3.0), 0.0)
        } else if sitting {
            body_c + vec2(s * 6.0, -bh / 2.0 - 4.0)
        } else {
            body_c + vec2(s * (bw / 2.0 - 2.0), -bh / 2.0 - 3.0)
        };
        let ear = |dx: f32| {
            p.add(Shape::convex_polygon(
                vec![
                    head + vec2(dx - 4.0, -5.0),
                    head + vec2(dx + 0.5 * s, -14.0),
                    head + vec2(dx + 4.0, -5.0),
                ],
                DARK,
                Stroke::NONE,
            ));
        };
        ear(-4.5);
        ear(4.5);
        p.circle_filled(head, 9.0, BODY);

        let blink = (t % 4.3) > 4.18;
        for dx in [s * 1.5, s * 6.5] {
            let e = head + vec2(dx, -1.0);
            if sleeping || blink {
                p.line_segment([e + vec2(-2.0, 0.0), e + vec2(2.0, 0.0)], Stroke::new(1.4, Color32::from_black_alpha(200)));
            } else {
                p.circle_filled(e, 1.9, Color32::from_black_alpha(220));
            }
        }
        p.circle_filled(head + vec2(s * 9.0, 2.5), 1.2, Color32::from_rgb(255, 120, 170));

        if sleeping {
            for i in 0..3 {
                let k = ((t * 0.5 + i as f32 / 3.0) % 1.0).clamp(0.0, 1.0);
                let a = (1.0 - k) * 200.0;
                p.text(
                    head + vec2(s * (8.0 + k * 12.0), -12.0 - k * 22.0),
                    Align2::CENTER_CENTER,
                    "z",
                    FontId::proportional(9.0 + 4.0 * k),
                    Color32::from_white_alpha(a as u8),
                );
            }
        }
    }
}
