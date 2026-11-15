use eframe::egui::{
    self, layers::ShapeIdx, pos2, vec2, Align2, Color32, Id, Painter, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2,
};
use egui_glass::GlassStyle;

use std::path::Path;

use crate::applets::{bold, Applet};
use crate::glass;
use crate::settings::Hub;
use crate::store::{AppletSave, TileSave};

pub type Cell = (usize, usize, usize, usize);

const HEADER: f32 = 22.0;
const TITLE_INSET: Vec2 = vec2(14.0, 6.0);

fn top_offset(titled: bool) -> f32 {
    if titled {
        HEADER + 14.0
    } else {
        0.0
    }
}

pub struct View {
    pub expanded: bool,
    pub editing: bool,
    pub ctrl: bool,
}

#[derive(Default)]
pub struct Out {
    pub body_hovered: bool,
    pub expand: bool,
    pub long_press: bool,
    pub remove: bool,
    pub drag: Option<Vec2>,
    pub drag_end: bool,
    pub resize: Option<Vec2>,
    pub resize_end: bool,
}

pub struct Tile {
    pub id: u64,
    pub cell: Cell,
    applets: Vec<Box<dyn Applet>>,
    active: usize,
    scroll_acc: f32,
    last_switch: f64,
}

impl Tile {
    pub fn restore(save: &TileSave, hub: &Hub, ctx: &egui::Context, dir: &Path) -> Option<Self> {
        let applets: Vec<Box<dyn Applet>> = save
            .applets
            .iter()
            .map(|s| {
                let mut applet = s.kind.build(hub);
                if let Some(data) = &s.data {
                    applet.restore(ctx, dir, data);
                }
                applet
            })
            .collect();
        if applets.is_empty() {
            return None;
        }
        let c = save.cell;
        let mut tile = Self::new(save.id, (c[0], c[1], c[2], c[3]), applets);
        tile.active = save.active.min(tile.applets.len() - 1);
        Some(tile)
    }

    pub fn snapshot(&mut self, dir: &Path) -> TileSave {
        let c = self.cell;
        TileSave {
            id: self.id,
            cell: [c.0, c.1, c.2, c.3],
            active: self.active,
            applets: self
                .applets
                .iter_mut()
                .map(|a| AppletSave { kind: a.kind(), data: a.save(dir) })
                .collect(),
        }
    }

    pub fn new(id: u64, cell: Cell, applets: Vec<Box<dyn Applet>>) -> Self {
        Self {
            id,
            cell,
            applets,
            active: 0,
            scroll_acc: 0.0,
            last_switch: 0.0,
        }
    }

    fn capsule(&self) -> bool {
        self.cell.3 == 1 && self.cell.2 >= 2
    }

    fn pad(&self, rect: Rect) -> f32 {
        if self.capsule() {
            (rect.height() * 0.14).clamp(16.0, 30.0)
        } else {
            16.0
        }
    }

    pub fn ghost(&mut self, ui: &mut Ui, rect: Rect, radius: f32) {
        let radius = if self.capsule() { (rect.height() / 2.0).min(90.0) } else { radius };
        ui.painter().rect_filled(rect, radius, Color32::from_white_alpha(20));
        ui.painter()
            .rect_stroke(rect, radius, Stroke::new(1.0, Color32::from_white_alpha(44)), StrokeKind::Inside);
        if rect.width() < 70.0 || rect.height() < 70.0 {
            return;
        }
        let inner = rect.shrink(self.pad(rect));
        let top = top_offset(self.applets[self.active].titled());
        let mut body = Rect::from_min_max(pos2(inner.min.x, inner.min.y + top), inner.max);
        if self.applets.len() > 1 {
            body.max.y -= 14.0;
        }
        let mut child = ui.new_child(UiBuilder::new().id_salt((self.id, "ghost")).max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        self.applets[self.active].ghost(&mut child);
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

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, glass: Rect, style: &GlassStyle, view: &View) -> Out {
        let ctx = ui.ctx().clone();
        let layer = ui.layer_id();
        let start = ctx.graphics_mut(|g| g.entry(layer).next_idx().0);
        let mut out = Out::default();

        let capsule = self.capsule();
        let mut style = *style;
        if capsule {
            style.corner_radius = (glass.height() / 2.0).min(90.0);
        }
        egui_glass::paint_glass(ui, glass, &style);

        if rect.width() < 70.0 || rect.height() < 70.0 {
            return out;
        }

        let hovering = !view.editing && ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p));

        if !view.editing {
            let body = ui.interact(rect, Id::unique((self.id, "body")), Sense::click_and_drag());
            out.body_hovered = body.hovered();
            if body.is_pointer_button_down_on() && !body.dragged() {
                ctx.request_repaint();
                let held = ui.input(|i| i.pointer.press_start_time().map_or(0.0, |t| i.time - t));
                if held > 0.55 {
                    out.long_press = true;
                }
            }
        }

        let inner = rect.shrink(self.pad(rect));
        let titled = self.applets[self.active].titled();
        if titled {
            ui.painter().text(
                inner.left_top() + vec2(TITLE_INSET.x, HEADER / 2.0 + TITLE_INSET.y),
                Align2::LEFT_CENTER,
                self.applets[self.active].name(),
                bold(12.5),
                Color32::from_white_alpha(200),
            );
        }

        let many = self.applets.len() > 1;

        let top = top_offset(titled);
        let mut body = Rect::from_min_max(pos2(inner.min.x, inner.min.y + top), inner.max);
        if many {
            body.max.y -= 14.0;
        }

        let pos = ctx.animate_value_with_time(Id::unique((self.id, "slide")), self.active as f32, 0.25);
        for (i, applet) in self.applets.iter_mut().enumerate() {
            let off = i as f32 - pos;
            if off.abs() >= 1.0 {
                continue;
            }
            let r = body.translate(vec2(off * body.width(), 0.0));
            let mut child = ui.new_child(UiBuilder::new().id_salt((self.id, i)).max_rect(r));
            child.set_clip_rect(body.intersect(ui.clip_rect()));
            child.set_opacity(1.0 - off.abs());
            applet.ui(&mut child);
        }

        let menu_open = egui::Popup::is_any_open(&ctx);
        let fade = ctx.animate_bool_with_time(Id::unique((self.id, "hover")), hovering || menu_open, 0.15);
        if fade > 0.02 {
            let expand_rect = Rect::from_min_size(
                pos2(inner.max.x - HEADER - 6.0, inner.min.y + TITLE_INSET.y),
                vec2(HEADER, HEADER),
            );
            let resp = icon_button(ui, expand_rect, (self.id, "expand"), fade, |p, r, c| {
                expand_icon(p, r, c, view.expanded)
            });
            out.expand = resp.clicked();

            if many {
                let drop_rect = expand_rect.translate(vec2(-HEADER - 4.0, 0.0));
                let resp = icon_button(ui, drop_rect, (self.id, "drop"), fade, chevron_icon);
                let names: Vec<&'static str> = self.applets.iter().map(|a| a.name()).collect();
                let mut pick = None;
                glass::menu(&resp, |ui| {
                    for (i, name) in names.iter().enumerate() {
                        if ui.selectable_label(i == self.active, *name).clicked() {
                            pick = Some(i);
                            ui.close();
                        }
                    }
                });
                if let Some(i) = pick {
                    self.active = i;
                }
            }
        }

        if many {
            let show = ctx.animate_bool_with_time(Id::unique((self.id, "dots")), view.ctrl && hovering, 0.15);
            if show > 0.02 {
                let n = self.applets.len();
                for i in 0..n {
                    let x = inner.center().x + (i as f32 - (n as f32 - 1.0) / 2.0) * 13.0;
                    let near = (1.0 - (i as f32 - pos).abs()).clamp(0.0, 1.0);
                    let a = (90.0 + 165.0 * near) * show;
                    ui.painter()
                        .circle_filled(pos2(x, inner.max.y - 4.0), 2.6 + near, Color32::from_white_alpha(a as u8));
                }
            }
        }

        if view.editing {
            let drag = ui
                .interact(rect, Id::unique((self.id, "drag")), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Grab);
            if drag.dragged() {
                out.drag = Some(drag.drag_delta());
            }
            out.drag_end = drag.drag_stopped();

            let handle = Rect::from_min_size(rect.max - vec2(34.0, 34.0), vec2(28.0, 28.0));
            let size = ui
                .interact(handle, Id::unique((self.id, "resize")), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            if size.dragged() {
                out.resize = Some(size.drag_delta());
            }
            out.resize_end = size.drag_stopped();
            let g = handle.center();
            let s = Stroke::new(1.6, Color32::from_white_alpha(220));
            ui.painter().line_segment([g + vec2(-5.0, 6.0), g + vec2(6.0, -5.0)], s);
            ui.painter().line_segment([g + vec2(1.0, 6.0), g + vec2(6.0, 1.0)], s);

            let badge = Rect::from_center_size(rect.min + vec2(16.0, 16.0), vec2(26.0, 26.0));
            let remove = ui
                .interact(badge, Id::unique((self.id, "remove")), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().circle_filled(badge.center(), 10.0, Color32::from_white_alpha(235));
            ui.painter().line_segment(
                [badge.center() + vec2(-4.0, 0.0), badge.center() + vec2(4.0, 0.0)],
                Stroke::new(2.0, Color32::from_black_alpha(200)),
            );
            out.remove = remove.clicked();
        }

        add_text_shadows(&ctx, layer, start);
        out
    }
}

fn add_text_shadows(ctx: &egui::Context, layer: egui::LayerId, start: usize) {
    ctx.graphics_mut(|g| {
        let list = g.entry(layer);
        let texts: Vec<_> = list
            .all_entries()
            .enumerate()
            .skip(start)
            .filter_map(|(i, cs)| match &cs.shape {
                Shape::Text(t) => Some((i, cs.clip_rect, t.clone())),
                _ => None,
            })
            .collect();
        for (i, clip, text) in texts {
            let mut shadow = text.clone();
            shadow.pos += vec2(0.0, 1.0);
            shadow.override_text_color = Some(Color32::from_black_alpha(85));
            list.set(ShapeIdx(i), clip, Shape::Vec(vec![Shape::Text(shadow), Shape::Text(text)]));
        }
    });
}

fn icon_button(
    ui: &mut Ui,
    rect: Rect,
    id: (u64, &'static str),
    fade: f32,
    draw: impl FnOnce(&Painter, Rect, Color32),
) -> egui::Response {
    let resp = ui
        .interact(rect, Id::unique(id), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, rect.height() / 2.0, Color32::from_white_alpha((30.0 * fade) as u8));
    }
    let alpha = if resp.hovered() { 255.0 } else { 175.0 };
    draw(ui.painter(), rect.shrink(7.0), Color32::from_white_alpha((alpha * fade) as u8));
    resp
}

fn expand_icon(p: &Painter, r: Rect, c: Color32, expanded: bool) {
    let s = Stroke::new(1.5, c);
    p.line_segment([r.left_bottom(), r.right_top()], s);
    let (a, b) = if expanded {
        (r.center().lerp(r.right_top(), 0.2), r.center().lerp(r.left_bottom(), 0.2))
    } else {
        (r.right_top(), r.left_bottom())
    };
    let k = if expanded { -1.0 } else { 1.0 };
    p.add(Shape::line(vec![pos2(a.x - 4.5 * k, a.y), a, pos2(a.x, a.y + 4.5 * k)], s));
    p.add(Shape::line(vec![pos2(b.x + 4.5 * k, b.y), b, pos2(b.x, b.y - 4.5 * k)], s));
}

fn chevron_icon(p: &Painter, r: Rect, c: Color32) {
    let m = r.center();
    p.add(Shape::line(
        vec![pos2(m.x - 4.0, m.y - 2.0), pos2(m.x, m.y + 2.0), pos2(m.x + 4.0, m.y - 2.0)],
        Stroke::new(1.5, c),
    ));
}