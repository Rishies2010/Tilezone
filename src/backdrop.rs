use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver, TrySendError};
use std::time::Duration;

use eframe::egui::epaint::Vertex;
use eframe::egui::{self, pos2, vec2, Color32, Mesh, Rect, Shape, TextureId, Ui};

use crate::warp::Warp;
use eframe::egui_wgpu::{wgpu, RenderState};
use image::AnimationDecoder;

const VIDEO: (u32, u32) = (1280, 720);
const AURORA: (u32, u32) = (256, 144);
const STILL_MAX: u32 = 3840;
const CLIP_MAX: u32 = 1280;
const CLIP_BUDGET: usize = 384 << 20;
const VIDEO_EXT: [&str; 5] = ["mp4", "mkv", "webm", "mov", "avi"];

struct Gpu {
    tex: wgpu::Texture,
    id: TextureId,
    size: (u32, u32),
}

struct Clip {
    size: (u32, u32),
    frames: Vec<Vec<u8>>,
    ends: Vec<f64>,
    total: f64,
}

struct Video {
    child: Child,
    rx: Receiver<Vec<u8>>,
    shown: bool,
}

impl Video {
    fn open(path: &Path) -> Result<Self, String> {
        let (w, h) = VIDEO;
        let filter = format!("fps=30,scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}");
        let mut child = Command::new("ffmpeg")
            .args(["-v", "error", "-re", "-stream_loop", "-1", "-i"])
            .arg(path)
            .args(["-an", "-sn", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ffmpeg was not found on PATH".to_string())?;
        let mut out = child.stdout.take().ok_or("ffmpeg gave no output pipe")?;
        let (tx, rx) = sync_channel(3);
        let len = (w * h * 4) as usize;
        std::thread::spawn(move || loop {
            let mut buf = vec![0u8; len];
            if out.read_exact(&mut buf).is_err() {
                break;
            }
            if let Err(TrySendError::Disconnected(_)) = tx.try_send(buf) {
                break;
            }
        });
        Ok(Self { child, rx, shown: false })
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Source {
    Still,
    Clip(Clip, Option<usize>),
    Video(Video),
    Aurora,
}

pub struct Wallpaper {
    rs: RenderState,
    gpu: Option<Gpu>,
    source: Source,
    label: String,
    error: Option<String>,
}

impl Wallpaper {
    pub fn new(rs: &RenderState) -> Self {
        let mut w = Self {
            rs: rs.clone(),
            gpu: None,
            source: Source::Aurora,
            label: "Aurora".into(),
            error: None,
        };
        w.ensure(AURORA);
        w
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub fn use_aurora(&mut self) {
        self.ensure(AURORA);
        self.source = Source::Aurora;
        self.label = "Aurora".into();
    }

    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());

        if VIDEO_EXT.contains(&ext.as_str()) {
            let video = Video::open(path)?;
            self.ensure(VIDEO);
            self.source = Source::Video(video);
        } else if let Some(clip) = clip(path, &ext)? {
            self.ensure(clip.size);
            self.source = Source::Clip(clip, None);
        } else {
            let mut img = image::open(path).map_err(|e| e.to_string())?;
            if img.width().max(img.height()) > STILL_MAX {
                img = img.thumbnail(STILL_MAX, STILL_MAX);
            }
            let rgba = img.to_rgba8();
            self.ensure((rgba.width(), rgba.height()));
            upload(&self.rs, &self.gpu, rgba.as_raw());
            self.source = Source::Still;
        }
        self.label = name;
        Ok(())
    }

    fn ensure(&mut self, size: (u32, u32)) {
        if self.gpu.as_ref().is_some_and(|g| g.size == size) {
            return;
        }
        if let Some(old) = self.gpu.take() {
            egui_glass::free_native_texture_id(&self.rs, &old.id);
        }
        let tex = self.rs.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tilezone_wallpaper"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let id = egui_glass::register_native_texture(&self.rs, &view);
        self.gpu = Some(Gpu { tex, id, size });
    }

    pub fn update(&mut self, ctx: &egui::Context, time: f64, fps: u32) {
        let interval = 1.0 / fps.max(1) as f32;
        let mut failed = false;
        match &mut self.source {
            Source::Still => {}
            Source::Aurora => {
                upload(&self.rs, &self.gpu, &aurora(AURORA, time as f32));
                ctx.request_repaint_after(Duration::from_secs_f32(interval));
            }
            Source::Clip(clip, shown) => {
                let t = time % clip.total;
                let idx = clip.ends.partition_point(|&e| e <= t).min(clip.frames.len() - 1);
                if *shown != Some(idx) {
                    upload(&self.rs, &self.gpu, &clip.frames[idx]);
                    *shown = Some(idx);
                }
                let next = (clip.ends[idx] - t).max(0.0) as f32;
                ctx.request_repaint_after(Duration::from_secs_f32(next.max(interval)));
            }
            Source::Video(v) => {
                let mut latest = None;
                while let Ok(frame) = v.rx.try_recv() {
                    latest = Some(frame);
                }
                if let Some(frame) = latest {
                    upload(&self.rs, &self.gpu, &frame);
                    v.shown = true;
                }
                if !v.shown && matches!(v.child.try_wait(), Ok(Some(_))) {
                    failed = true;
                }
                ctx.request_repaint_after(Duration::from_secs_f32(interval));
            }
        }
        if failed {
            self.error = Some("ffmpeg could not play that file".into());
            self.use_aurora();
        }
    }

    pub fn image(&self, rect: Rect) -> Option<(TextureId, Rect)> {
        let g = self.gpu.as_ref()?;
        let size = vec2(g.size.0 as f32, g.size.1 as f32);
        let scale = (rect.width() / size.x).max(rect.height() / size.y);
        Some((g.id, Rect::from_center_size(rect.center(), size * scale)))
    }

    pub fn paint(&self, ui: &Ui, rect: Rect, warp: &Warp, time: f64) {
        let Some((id, img)) = self.image(rect) else { return };
        let painter = ui.painter().with_clip_rect(rect);
        if !warp.active() {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(id, img, uv, Color32::WHITE);
            return;
        }
        let (cols, rows) = (48usize, 27usize);
        let area = rect.expand(56.0);
        let mut mesh = Mesh::with_texture(id);
        for j in 0..=rows {
            for i in 0..=cols {
                let q = pos2(
                    area.min.x + area.width() * i as f32 / cols as f32,
                    area.min.y + area.height() * j as f32 / rows as f32,
                );
                let uv = pos2(
                    ((q.x - img.min.x) / img.width()).clamp(0.0, 1.0),
                    ((q.y - img.min.y) / img.height()).clamp(0.0, 1.0),
                );
                mesh.vertices.push(Vertex { pos: q + warp.field(q, time), uv, color: Color32::WHITE });
            }
        }
        let w = (cols + 1) as u32;
        for j in 0..rows as u32 {
            for i in 0..cols as u32 {
                let a = j * w + i;
                mesh.indices.extend_from_slice(&[a, a + 1, a + w, a + 1, a + w + 1, a + w]);
            }
        }
        painter.add(Shape::mesh(mesh));
    }
}

fn upload(rs: &RenderState, gpu: &Option<Gpu>, data: &[u8]) {
    let Some(g) = gpu else { return };
    if data.len() != (g.size.0 * g.size.1 * 4) as usize {
        return;
    }
    rs.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &g.tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * g.size.0), rows_per_image: Some(g.size.1) },
        wgpu::Extent3d { width: g.size.0, height: g.size.1, depth_or_array_layers: 1 },
    );
}

fn clip(path: &Path, ext: &str) -> Result<Option<Clip>, String> {
    if ext != "gif" && ext != "webp" {
        return Ok(None);
    }
    let reader = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let frames = if ext == "gif" {
        image::codecs::gif::GifDecoder::new(reader).map_err(|e| e.to_string())?.into_frames()
    } else {
        let decoder = image::codecs::webp::WebPDecoder::new(reader).map_err(|e| e.to_string())?;
        if !decoder.has_animation() {
            return Ok(None);
        }
        decoder.into_frames()
    };

    let mut clip = Clip { size: (0, 0), frames: Vec::new(), ends: Vec::new(), total: 0.0 };
    let mut used = 0usize;
    for frame in frames {
        let frame = frame.map_err(|e| e.to_string())?;
        let (num, den) = frame.delay().numer_denom_ms();
        let secs = if num == 0 { 0.1 } else { (num as f64 / den.max(1) as f64 / 1000.0).max(0.02) };
        let mut img = image::DynamicImage::ImageRgba8(frame.into_buffer());
        if img.width().max(img.height()) > CLIP_MAX {
            img = img.thumbnail(CLIP_MAX, CLIP_MAX);
        }
        let rgba = img.to_rgba8();
        let size = (rgba.width(), rgba.height());
        if clip.frames.is_empty() {
            clip.size = size;
        } else if size != clip.size {
            break;
        }
        used += rgba.as_raw().len();
        if used > CLIP_BUDGET && !clip.frames.is_empty() {
            break;
        }
        clip.total += secs;
        clip.ends.push(clip.total);
        clip.frames.push(rgba.into_raw());
    }
    Ok((clip.frames.len() > 1).then_some(clip))
}

struct Blob {
    color: [f32; 3],
    amp: [f32; 2],
    speed: [f32; 2],
    phase: f32,
    radius: f32,
}

const BLOBS: [Blob; 6] = [
    Blob { color: [0.85, 0.25, 0.80], amp: [0.38, 0.30], speed: [0.13, 0.17], phase: 0.0, radius: 0.30 },
    Blob { color: [0.55, 0.08, 1.00], amp: [0.42, 0.28], speed: [0.11, 0.15], phase: 2.1, radius: 0.34 },
    Blob { color: [1.00, 0.12, 0.55], amp: [0.30, 0.35], speed: [0.19, 0.12], phase: 4.0, radius: 0.22 },
    Blob { color: [0.12, 0.15, 0.95], amp: [0.40, 0.25], speed: [0.09, 0.14], phase: 5.3, radius: 0.38 },
    Blob { color: [0.45, 0.20, 0.90], amp: [0.35, 0.32], speed: [0.16, 0.10], phase: 1.0, radius: 0.26 },
    Blob { color: [0.05, 0.28, 0.42], amp: [0.45, 0.30], speed: [0.07, 0.12], phase: 3.3, radius: 0.20 },
];

fn aurora((w, h): (u32, u32), t: f32) -> Vec<u8> {
    let centers: Vec<[f32; 2]> = BLOBS
        .iter()
        .map(|b| {
            [
                0.5 + b.amp[0] * (t * b.speed[0] + b.phase).sin(),
                0.5 + b.amp[1] * (t * b.speed[1] + b.phase * 1.7 + 1.0).sin(),
            ]
        })
        .collect();
    let aspect = w as f32 / h as f32;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let v = y as f32 / (h - 1) as f32;
        for x in 0..w {
            let u = x as f32 / (w - 1) as f32;
            let mut lin = [0.025f32, 0.010, 0.060];
            for (b, c) in BLOBS.iter().zip(&centers) {
                let dx = (u - c[0]) * aspect;
                let dy = v - c[1];
                let k = (-(dx * dx + dy * dy) / (b.radius * b.radius)).exp() * 0.85;
                for i in 0..3 {
                    lin[i] += b.color[i] * k;
                }
            }
            let vignette = 1.0 - 0.7 * ((u - 0.5).powi(2) + (v - 0.5).powi(2));
            for c in lin {
                let mapped = c / (1.0 + 0.6 * c) * vignette;
                out.push((mapped.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
            }
            out.push(255);
        }
    }
    out
}
