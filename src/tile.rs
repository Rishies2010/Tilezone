use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Id, Painter, Rect, Sense, Shape, Stroke, Ui, UiBuilder};
use egui_glass::GlassStyle;

use crate::applets::Applet;

const PAD: f32 = 20.0;
const HEADER: f32 = 26.0;

pub struct Tile {
    pub id: &'static str,
    pub cell: (usize, usize, usize, usize),
    applets: Vec<Box<dyn Applet>>,
    active: usize,
    scroll_acc: f32,
    last_switch: f64,
}

impl Tile {
    pub fn new(id: &'static str, cell: (usize, usize, usize, usize), applets: Vec<Box<dyn Applet>>) -> Self {
        Self {
            id,
            cell,
            applets,
            active: 0,
            scroll_acc: 0.0,
            last_switch: 0.0,
        }
    }

    pub fn scroll(&mut self, amount: f32, now: f64) {
        if self.applets.len() < 2 || now - self.last_switch < 0.2 {
            return;
        }
        self.scroll_acc += amount;
        if self.scroll_acc.abs() >= 1.0 {
            let n = self.applets.len();
            self.active = if self.scroll_acc < 0.0 {
                (self.active + 1) % n
            } else {
                (self.active + n - 1) % n
            };
            self.scroll_acc = 0.0;
            self.last_switch = now;
        }
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, style: &GlassStyle, expanded: bool) -> bool {
        egui_glass::paint_glass(ui, rect, style);

        let inner = rect.shrink(PAD);
        let name = self.applets[self.active].name();
        ui.painter().text(
            inner.left_top() + vec2(0.0, HEADER / 2.0),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(15.0),
            Color32::from_white_alpha(190),
        );

        let expand_rect = Rect::from_min_size(pos2(inner.max.x - HEADER, inner.min.y), vec2(HEADER, HEADER));
        let toggle = icon_button(ui, expand_rect, (self.id, "expand"), |p, r, c| expand_icon(p, r, c, expanded))
            .clicked();

        let many = self.applets.len() > 1;
        if many {
            let drop_rect = expand_rect.translate(vec2(-HEADER - 4.0, 0.0));
            let resp = icon_button(ui, drop_rect, (self.id, "drop"), chevron_icon);
            let mut pick = None;
            egui::Popup::menu(&resp).show(|ui| {
                for (i, a) in self.applets.iter().enumerate() {
                    if ui.selectable_label(i == self.active, a.name()).clicked() {
                        pick = Some(i);
                        ui.close();
                    }
                }
            });
            if let Some(i) = pick {
                self.active = i;
            }
        }

        let mut body = Rect::from_min_max(pos2(inner.min.x, inner.min.y + HEADER + 10.0), inner.max);
        if many {
            body.max.y -= 18.0;
        }

        let pos = ui
            .ctx()
            .animate_value_with_time(Id::unique((self.id, "slide")), self.active as f32, 0.25);
        for (i, applet) in self.applets.iter_mut().enumerate() {
            let off = i as f32 - pos;
            if off.abs() >= 1.0 {
                continue;
            }
            let r = body.translate(vec2(off * body.width(), 0.0));
            let mut child = ui.new_child(UiBuilder::new().max_rect(r));
            child.set_clip_rect(body.intersect(ui.clip_rect()));
            child.set_opacity(1.0 - off.abs());
            applet.ui(&mut child);
        }

        if many {
            let n = self.applets.len();
            for i in 0..n {
                let x = inner.center().x + (i as f32 - (n as f32 - 1.0) / 2.0) * 14.0;
                let near = (1.0 - (i as f32 - pos).abs()).clamp(0.0, 1.0);
                ui.painter().circle_filled(
                    pos2(x, inner.max.y - 5.0),
                    3.0 + near,
                    Color32::from_white_alpha((90.0 + 140.0 * near) as u8),
                );
            }
        }

        toggle
    }
}

fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    id: (&'static str, &'static str),
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> egui::Response {
    let resp = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.hovered() {
        ui.painter().rect_filled(rect, rect.height() / 2.0, Color32::from_white_alpha(28));
    }
    let alpha = if resp.hovered() { 255 } else { 170 };
    draw(ui.painter(), rect.shrink(8.0), Color32::from_white_alpha(alpha));
    resp
}

fn expand_icon(p: &Painter, r: Rect, c: Color32, expanded: bool) {
    let s = Stroke::new(1.6, c);
    p.line_segment([r.left_bottom(), r.right_top()], s);
    let (a, b) = if expanded {
        (r.center().lerp(r.right_top(), 0.2), r.center().lerp(r.left_bottom(), 0.2))
    } else {
        (r.right_top(), r.left_bottom())
    };
    let k = if expanded { -1.0 } else { 1.0 };
    p.add(Shape::line(vec![pos2(a.x - 5.0 * k, a.y), a, pos2(a.x, a.y + 5.0 * k)], s));
    p.add(Shape::line(vec![pos2(b.x + 5.0 * k, b.y), b, pos2(b.x, b.y - 5.0 * k)], s));
}

fn chevron_icon(p: &Painter, r: Rect, c: Color32) {
    let m = r.center();
    p.add(Shape::line(
        vec![pos2(m.x - 4.5, m.y - 2.0), pos2(m.x, m.y + 2.5), pos2(m.x + 4.5, m.y - 2.0)],
        Stroke::new(1.6, c),
    ));
}