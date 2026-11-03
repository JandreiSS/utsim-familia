//! Headless picture of an egui frame: runs the UI with no window, tessellates it and fills
//! the triangles in software into an RGBA image, which can be saved as a PNG. For tests and
//! for reviewing a layout without a screen (slow, not for real use).

use std::collections::HashMap;
use std::path::Path;

use egui::epaint::{ClippedPrimitive, Primitive, TextureId};
use egui::{Color32, ColorImage, Context, ImageData, RawInput, Rect, Ui, Vec2, pos2};

/// An image of `width` x `height` RGBA pixels (premultiplied while drawing, saved straight).
#[derive(Debug, Clone)]
pub struct Imagem {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<[f32; 4]>,
}

impl Imagem {
    /// The pixel at (x, y), 8 bits per channel.
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let p = self.rgba[y * self.width + x];
        p.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
    }

    /// Writes the image as a PNG.
    pub fn salvar_png(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = png::Encoder::new(file, self.width as u32, self.height as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(std::io::Error::other)?;
        let mut data = Vec::with_capacity(self.width * self.height * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let mut p = self.pixel(x, y);
                p[3] = 255;
                data.extend_from_slice(&p);
            }
        }
        w.write_image_data(&data).map_err(std::io::Error::other)
    }
}

/// Runs frames of a UI and keeps the textures egui uploads.
pub struct Captura {
    pub ctx: Context,
    textures: HashMap<TextureId, ColorImage>,
    /// Size of the "window", points.
    pub size: Vec2,
    pub pixels_per_point: f32,
    /// Time of the next frame, seconds.
    pub time: f64,
}

impl Captura {
    pub fn new(ctx: Context, size: Vec2) -> Self {
        Self {
            ctx,
            textures: HashMap::new(),
            size,
            pixels_per_point: 1.0,
            time: 0.0,
        }
    }

    fn input(&self, events: Vec<egui::Event>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), self.size)),
            time: Some(self.time),
            events,
            ..RawInput::default()
        }
    }

    /// Runs one frame (with optional input events) and returns its primitives.
    pub fn quadro(
        &mut self,
        events: Vec<egui::Event>,
        run: impl FnMut(&mut Ui),
    ) -> Vec<ClippedPrimitive> {
        self.ctx.set_pixels_per_point(self.pixels_per_point);
        let mut out = self.ctx.run_ui(self.input(events), run);
        for (id, deltas) in std::mem::take(&mut out.textures_delta.set) {
            for delta in deltas {
                let ImageData::Color(img) = &delta.image;
                match delta.pos {
                    None => {
                        self.textures.insert(id, (**img).clone());
                    }
                    Some([x0, y0]) => {
                        if let Some(t) = self.textures.get_mut(&id) {
                            for y in 0..img.size[1] {
                                for x in 0..img.size[0] {
                                    let (tx, ty) = (x0 + x, y0 + y);
                                    if tx < t.size[0] && ty < t.size[1] {
                                        t.pixels[ty * t.size[0] + tx] =
                                            img.pixels[y * img.size[0] + x];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for id in std::mem::take(&mut out.textures_delta.free) {
            self.textures.remove(&id);
        }
        let shapes = std::mem::take(&mut out.shapes);
        let ppp = out.pixels_per_point;
        self.time += 1.0 / 60.0;
        self.ctx.tessellate(shapes, ppp)
    }

    /// Runs `warmup` frames (layout settles) and paints the last one.
    pub fn imagem(&mut self, warmup: usize, mut run: impl FnMut(&mut Ui)) -> Imagem {
        let mut prims = Vec::new();
        for _ in 0..warmup.max(1) {
            prims = self.quadro(Vec::new(), &mut run);
        }
        self.pintar(&prims)
    }

    /// Fills the primitives into an image of the window's size.
    pub fn pintar(&self, prims: &[ClippedPrimitive]) -> Imagem {
        let ppp = self.pixels_per_point;
        let width = (self.size.x * ppp).round() as usize;
        let height = (self.size.y * ppp).round() as usize;
        let mut img = Imagem {
            width,
            height,
            rgba: vec![[0.0, 0.0, 0.0, 1.0]; width * height],
        };
        for prim in prims {
            let Primitive::Mesh(mesh) = &prim.primitive else {
                continue;
            };
            let Some(tex) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let clip = prim.clip_rect;
            let cx0 = (clip.min.x * ppp).max(0.0) as i64;
            let cy0 = (clip.min.y * ppp).max(0.0) as i64;
            let cx1 = ((clip.max.x * ppp).ceil() as i64).min(width as i64);
            let cy1 = ((clip.max.y * ppp).ceil() as i64).min(height as i64);
            for tri in mesh.indices.as_chunks::<3>().0 {
                let v = [
                    &mesh.vertices[tri[0] as usize],
                    &mesh.vertices[tri[1] as usize],
                    &mesh.vertices[tri[2] as usize],
                ];
                let p = v.map(|v| (v.pos.x * ppp, v.pos.y * ppp));
                let area =
                    (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0);
                if area.abs() < 1e-6 {
                    continue;
                }
                let minx = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i64).max(cx0);
                let maxx = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i64).min(cx1);
                let miny = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i64).max(cy0);
                let maxy = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i64).min(cy1);
                for y in miny..maxy {
                    for x in minx..maxx {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let w0 =
                            ((p[1].0 - px) * (p[2].1 - py) - (p[1].1 - py) * (p[2].0 - px)) / area;
                        let w1 =
                            ((p[2].0 - px) * (p[0].1 - py) - (p[2].1 - py) * (p[0].0 - px)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                            continue;
                        }
                        let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                        let t = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                        let tx = ((u * tex.size[0] as f32) as usize).min(tex.size[0] - 1);
                        let ty = ((t * tex.size[1] as f32) as usize).min(tex.size[1] - 1);
                        let texel = tex.pixels[ty * tex.size[0] + tx];
                        let col =
                            |c: Color32| [c.r(), c.g(), c.b(), c.a()].map(|v| v as f32 / 255.0);
                        let (c0, c1, c2) = (col(v[0].color), col(v[1].color), col(v[2].color));
                        let tc = col(texel);
                        let mut src = [0.0f32; 4];
                        for k in 0..4 {
                            src[k] = (w0 * c0[k] + w1 * c1[k] + w2 * c2[k]) * tc[k];
                        }
                        let dst = &mut img.rgba[y as usize * width + x as usize];
                        for k in 0..4 {
                            dst[k] = src[k] + dst[k] * (1.0 - src[3]);
                        }
                    }
                }
            }
        }
        img
    }
}
