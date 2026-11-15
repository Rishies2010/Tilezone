use eframe::egui::{self, Color32, Id, LayerId, Order, Pos2, Stroke, Vec2};

const LIFE: f32 = 1.5;
const REACH: f32 = 720.0;
const WIDTH: f32 = 110.0;
const AMPLITUDE: f32 = 30.0;
const WELL: f32 = 0.5;

struct Shock {
    pos: Pos2,
    t0: f64,
    pull: bool,
}

#[derive(Default)]
pub struct Warp {
    shocks: Vec<Shock>,
}

impl Warp {
    pub fn push(&mut self, pos: Pos2, time: f64, pull: bool) {
        self.shocks.push(Shock { pos, t0: time, pull });
        if self.shocks.len() > 6 {
            self.shocks.remove(0);
        }
    }

    pub fn prune(&mut self, time: f64) {
        self.shocks.retain(|s| ((time - s.t0) as f32) < LIFE);
    }

    pub fn active(&self) -> bool {
        !self.shocks.is_empty()
    }

    fn front(age: f32) -> f32 {
        let k = (1.0 - age / LIFE).clamp(0.0, 1.0);
        REACH * (1.0 - k.powf(2.2))
    }

    pub fn field(&self, p: Pos2, time: f64) -> Vec2 {
        let mut out = Vec2::ZERO;
        for s in &self.shocks {
            let age = (time - s.t0) as f32;
            if !(0.0..LIFE).contains(&age) {
                continue;
            }
            let d = p - s.pos;
            let r = d.length().max(0.001);
            let dir = d / r;
            let sign = if s.pull { -1.0 } else { 1.0 };
            let fade = (1.0 - age / LIFE).powi(2);

            let x = (r - Self::front(age)) / WIDTH;
            let wave = x * (-x * x).exp() * 2.3 * AMPLITUDE * fade / (1.0 + r / 600.0);

            let bubble_t = (age / 0.7).clamp(0.0, 1.0);
            let pulse = (bubble_t * std::f32::consts::PI).sin();
            let well = -d * WELL * (-(r * r) / (2.0 * 120.0 * 120.0)).exp() * pulse;

            out += (dir * wave + well) * sign;
        }
        out
    }

    pub fn draw_rings(&self, ctx: &egui::Context, time: f64) {
        if self.shocks.is_empty() {
            return;
        }
        let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("warp_rings")));
        for s in &self.shocks {
            let age = (time - s.t0) as f32;
            if !(0.0..LIFE).contains(&age) {
                continue;
            }
            let fade = (1.0 - age / LIFE).powf(1.8);
            let r = Self::front(age).max(2.0);
            for (dr, tint, a) in [(-2.0, [255, 120, 200], 70.0), (0.0, [255, 255, 255], 120.0), (2.0, [110, 160, 255], 70.0)] {
                let c = Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], (a * fade) as u8);
                painter.circle_stroke(s.pos, (r + dr).max(1.0), Stroke::new(1.2, c));
            }
        }
    }
}
