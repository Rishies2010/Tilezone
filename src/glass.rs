use eframe::egui::{self, Color32, Context, Frame, Id, Margin, Response, Ui};
use egui_glass::{Glass, GlassStyle};

fn key() -> Id {
    Id::unique("tilezone::glass_style")
}

pub fn publish(ctx: &Context, style: GlassStyle) {
    ctx.data_mut(|d| d.insert_temp(key(), style));
}

pub fn current(ctx: &Context) -> GlassStyle {
    ctx.data(|d| d.get_temp(key())).unwrap_or_else(GlassStyle::clear)
}

pub fn popup_style(base: GlassStyle) -> GlassStyle {
    GlassStyle {
        corner_radius: 20.0,
        blur: 14.0,
        refraction: 16.0,
        edge_width: 14.0,
        tint: Color32::from_rgba_unmultiplied(255, 255, 255, 34),
        specular: 0.7,
        border: 0.6,
        shadow: 0.18,
        shadow_radius: 26.0,
        shadow_offset: 8.0,
        ..base
    }
}

fn panel(ui: &mut Ui, style: GlassStyle, add: impl FnOnce(&mut Ui)) {
    Glass::new(style).inner_margin(Margin::same(8)).show(ui, |ui| {
        ui.set_min_width(132.0);
        add(ui);
    });
}

pub fn menu(resp: &Response, add: impl FnOnce(&mut Ui)) {
    let style = popup_style(current(&resp.ctx));
    egui::Popup::menu(resp).frame(Frame::NONE).show(|ui| panel(ui, style, add));
}

pub fn context_menu(resp: &Response, add: impl FnOnce(&mut Ui)) {
    let style = popup_style(current(&resp.ctx));
    egui::Popup::context_menu(resp).frame(Frame::NONE).show(|ui| panel(ui, style, add));
}
