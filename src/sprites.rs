use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

pub const PET: &[u8] = include_bytes!("../assets/pet.png");
pub const APPLE: &[u8] = include_bytes!("../assets/apple.png");
pub const ENEMY: &[u8] = include_bytes!("../assets/enemy.png");

pub fn decode(bytes: &[u8]) -> ([usize; 2], Vec<u8>) {
    let img = image::load_from_memory(bytes).expect("embedded sprite is a valid image").to_rgba8();
    ([img.width() as usize, img.height() as usize], img.into_raw())
}

pub fn load(ctx: &Context, name: &str, bytes: &[u8], options: TextureOptions) -> TextureHandle {
    let (size, raw) = decode(bytes);
    ctx.load_texture(name, ColorImage::from_rgba_unmultiplied(size, &raw), options)
}
