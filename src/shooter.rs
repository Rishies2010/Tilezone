use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, pos2, vec2, Align2, Color32, Key, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

use crate::applets::{bold, dim, ghost_fill, Applet, ACCENT};

const W: f32 = 300.0;
const H: f32 = 400.0;
const PLAYER_Y: f32 = 366.0;
const PLAYER_SPEED: f32 = 280.0;

#[derive(PartialEq)]
enum Phase {
    Idle,
    Running,
    Dead,
}

struct Enemy {
    pos: Pos2,
    hp: u8,
    drift: f32,
    seed: f32,
}

struct Spark {
    pos: Pos2,
    vel: Vec2,
    life: f32,
}

pub struct Shooter {
    phase: Phase,
    x: f32,
    bullets: Vec<Pos2>,
    enemies: Vec<Enemy>,
    sparks: Vec<Spark>,
    stars: Vec<(Pos2, f32)>,
    fire: f32,
    spawn: f32,
    lives: u8,
    score: u32,
    best: u32,
    hit: f32,
    last: f64,
    rng: u64,
}

impl Shooter {
    pub fn new() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(11, |d| d.as_nanos() as u64) | 1;
        let mut s = Self {
            phase: Phase::Idle,
            x: W / 2.0,
            bullets: Vec::new(),
            enemies: Vec::new(),
            sparks: Vec::new(),
            stars: Vec::new(),
            fire: 0.0,
            spawn: 0.0,
            lives: 3,
            score: 0,
            best: 0,
            hit: 0.0,
            last: 0.0,
            rng: seed,
        };
        for _ in 0..40 {
            let star = (pos2(s.rand() * W, s.rand() * H), 0.3 + s.rand() * 0.7);
            s.stars.push(star);
        }
        s
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn start(&mut self, now: f64) {
        self.phase = Phase::Running;
        self.bullets.clear();
        self.enemies.clear();
        self.sparks.clear();
        self.x = W / 2.0;
        self.lives = 3;
        self.score = 0;
        self.fire = 0.0;
        self.spawn = 0.4;
        self.hit = 0.0;
        self.last = now;
    }

    fn burst(&mut self, at: Pos2, n: usize) {
        for _ in 0..n {
            let a = self.rand() * std::f32::consts::TAU;
            let v = 40.0 + self.rand() * 120.0;
            let life = 0.35 + self.rand() * 0.3;
            self.sparks.push(Spark { pos: at, vel: vec2(a.cos(), a.sin()) * v, life });
        }
    }

    fn lose_life(&mut self) {
        self.lives = self.lives.saturating_sub(1);
        self.hit = 0.4;
        if self.lives == 0 {
            self.phase = Phase::Dead;
            self.best = self.best.max(self.score);
        }
    }

    fn step(&mut self, dt: f32, steer: f32, pointer_x: Option<f32>) {
        match pointer_x {
            Some(px) => self.x += (px - self.x).clamp(-PLAYER_SPEED * dt * 1.6, PLAYER_SPEED * dt * 1.6),
            None => self.x += steer * PLAYER_SPEED * dt,
        }
        self.x = self.x.clamp(14.0, W - 14.0);
        self.hit = (self.hit - dt).max(0.0);

        self.fire -= dt;
        if self.fire <= 0.0 {
            self.fire = 0.2;
            self.bullets.push(pos2(self.x, PLAYER_Y - 16.0));
        }
        for b in &mut self.bullets {
            b.y -= 460.0 * dt;
        }
        self.bullets.retain(|b| b.y > -10.0);

        self.spawn -= dt;
        if self.spawn <= 0.0 {
            self.spawn = (0.95 - self.score as f32 * 0.012).max(0.3);
            let x = 20.0 + self.rand() * (W - 40.0);
            let hp = if self.score > 12 && self.rand() < 0.3 { 2 } else { 1 };
            let drift = (self.rand() - 0.5) * 60.0;
            let seed = self.rand() * 6.28;
            self.enemies.push(Enemy { pos: pos2(x, -16.0), hp, drift, seed });
        }

        let speed = 52.0 + self.score as f32 * 1.4;
        for e in &mut self.enemies {
            e.pos.y += speed * dt;
            e.seed += dt * 2.0;
            e.pos.x = (e.pos.x + (e.seed.sin() * 30.0 + e.drift) * dt).clamp(12.0, W - 12.0);
        }

        let mut kills = Vec::new();
        for e in &mut self.enemies {
            if let Some(i) = self.bullets.iter().position(|b| b.distance(e.pos) < 14.0) {
                self.bullets.swap_remove(i);
                e.hp = e.hp.saturating_sub(1);
                if e.hp == 0 {
                    kills.push(e.pos);
                }
            }
        }
        self.score += kills.len() as u32;
        for at in kills {
            self.burst(at, 10);
        }

        let player = pos2(self.x, PLAYER_Y);
        let mut lost = 0;
        let mut crashes = Vec::new();
        self.enemies.retain(|e| {
            if e.hp == 0 {
                return false;
            }
            if e.pos.distance(player) < 20.0 {
                crashes.push(e.pos);
                lost += 1;
                return false;
            }
            if e.pos.y > H + 10.0 {
                lost += 1;
                return false;
            }
            true
        });
        for at in crashes {
            self.burst(at, 14);
        }
        for _ in 0..lost {
            self.lose_life();
        }

        for s in &mut self.sparks {
            s.pos += s.vel * dt;
            s.life -= dt;
        }
        self.sparks.retain(|s| s.life > 0.0);
    }
}

impl Applet for Shooter {
    fn name(&self) -> &'static str {
        "Space shooter"
    }

    fn ghost(&mut self, ui: &mut Ui) {
        ghost_fill(ui);
    }

    fn ui(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let side = (rect.height() - 18.0).max(40.0);
        let scale = (rect.width() / W).min(side / H);
        let board = Rect::from_center_size(pos2(rect.center().x, rect.min.y + side / 2.0), vec2(W, H) * scale);
        let resp = ui.interact(board, ui.scope_id().with("shooter"), Sense::click());
        let now = ui.input(|i| i.time);
        let to_screen = |p: Pos2| board.min + p.to_vec2() * scale;

        if resp.clicked() && self.phase != Phase::Running {
            self.start(now);
        }

        if self.phase == Phase::Running {
            let dt = ((now - self.last) as f32).clamp(0.0, 0.05);
            self.last = now;
            let (steer, pointer) = ui.input(|i| {
                let mut s = 0.0;
                if i.key_down(Key::ArrowLeft) || i.key_down(Key::A) {
                    s -= 1.0;
                }
                if i.key_down(Key::ArrowRight) || i.key_down(Key::D) {
                    s += 1.0;
                }
                let p = if s == 0.0 && resp.hovered() { i.pointer.hover_pos() } else { None };
                (s, p.map(|p| (p.x - board.min.x) / scale))
            });
            self.step(dt, steer, pointer);
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        let p = ui.painter().with_clip_rect(board);
        p.rect_filled(board, 16.0, dim(16));
        let drift = if self.phase == Phase::Running { now as f32 * 30.0 } else { 0.0 };
        for (pos, depth) in &self.stars {
            let y = (pos.y + drift * depth).rem_euclid(H);
            p.circle_filled(to_screen(pos2(pos.x, y)), 0.6 + depth, dim((80.0 + 120.0 * depth) as u8));
        }

        for b in &self.bullets {
            let (a, c) = (to_screen(*b), to_screen(*b + vec2(0.0, 9.0)));
            p.line_segment([a, c], Stroke::new(2.4 * scale.max(0.6), Color32::WHITE));
        }
        for e in &self.enemies {
            let c = to_screen(e.pos);
            let r = 11.0 * scale;
            let color = if e.hp > 1 { Color32::from_rgb(255, 120, 190) } else { ACCENT };
            p.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)],
                color,
                Stroke::new(1.0, Color32::from_white_alpha(120)),
            ));
        }
        for s in &self.sparks {
            p.circle_filled(to_screen(s.pos), 1.8 * scale.max(0.6), dim((s.life * 380.0).min(235.0) as u8));
        }

        if self.phase != Phase::Dead && (self.hit <= 0.0 || (now * 30.0) as u32 % 2 == 0) {
            let c = to_screen(pos2(self.x, PLAYER_Y));
            let u = scale;
            p.add(Shape::convex_polygon(
                vec![
                    c + vec2(0.0, -16.0 * u),
                    c + vec2(12.0 * u, 12.0 * u),
                    c + vec2(0.0, 6.0 * u),
                    c + vec2(-12.0 * u, 12.0 * u),
                ],
                dim(240),
                Stroke::NONE,
            ));
        }

        let msg = match self.phase {
            Phase::Idle => Some("Click to play"),
            Phase::Dead => Some("Game over · click to retry"),
            Phase::Running => None,
        };
        if let Some(msg) = msg {
            p.rect_filled(board, 16.0, Color32::from_black_alpha(90));
            p.text(board.center(), Align2::CENTER_CENTER, msg, bold(14.0), Color32::WHITE);
        }

        let lives = "●".repeat(self.lives as usize);
        ui.painter().text(
            pos2(rect.center().x, rect.max.y - 8.0),
            Align2::CENTER_CENTER,
            format!("score {}  ·  best {}  ·  {}", self.score, self.best, lives),
            egui::FontId::proportional(11.0),
            dim(170),
        );
    }
}
