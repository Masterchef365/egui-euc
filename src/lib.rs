use std::collections::HashMap;

use egui::{
    epaint, ClippedPrimitive, Color32, ImageData, Rgba, TextureId, TextureOptions, TexturesDelta,
};
use euc::{Buffer2d, CullMode, Pipeline, Sampler, Target, Texture, TriangleList};

#[derive(Clone, Copy, Debug)]
pub struct EguiVertexData {
    pub uv: egui::Pos2,
    pub color: egui::Rgba,
}

impl std::ops::Mul<f32> for EguiVertexData {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            uv: self.uv.to_vec2().mul(rhs).to_pos2(),
            color: self.color.mul(rhs),
        }
    }
}

impl std::ops::Add<Self> for EguiVertexData {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            uv: self.uv + rhs.uv.to_vec2(),
            color: self.color + rhs.color,
        }
    }
}

impl From<epaint::Vertex> for EguiVertexData {
    fn from(value: epaint::Vertex) -> Self {
        EguiVertexData {
            uv: value.uv,
            color: value.color.into(),
        }
    }
}

pub struct EguiMeshEucPipeline<'r, S> {
    pub sampler: S,
    pub vertices: &'r [epaint::Vertex],
}

impl<'r, S> Pipeline<'r> for EguiMeshEucPipeline<'r, S>
//where
//S: Sampler<2, Index = f32, Sample = egui::Rgba>,
{
    type Vertex = u32;
    type VertexData = EguiVertexData;
    type Primitives = TriangleList;
    type Pixel = u32;
    type Fragment = Rgba;

    #[inline(always)]
    fn vertex(&self, idx: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        let vertex = self.vertices[*idx as usize];
        let xyzw = [vertex.pos.x / 100.0, vertex.pos.y / 100.0, 0.0, 1.0];
        (xyzw, vertex.into())
    }

    #[inline(always)]
    fn fragment(&self, color: Self::VertexData) -> Self::Fragment {
        color.color
    }

    fn blend(&self, _: Self::Pixel, color: Self::Fragment) -> Self::Pixel {
        u32::from_le_bytes(color.to_srgba_unmultiplied())
    }

    fn rasterizer_config(&self) -> CullMode {
        CullMode::None
    }
}

pub struct Scissor<T> {
    inner: T,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl<T> Scissor<T> {
    pub fn new(inner: T, x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            inner,
            x,
            y,
            width,
            height,
        }
    }

    fn bounds_check(&self, x: usize, y: usize) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    fn from_clip_rect(
        inner: T,
        [width_px, height_px]: [u32; 2],
        pixels_per_point: f32,
        clip_rect: egui::Rect,
    ) -> Self {
        // Transform clip rect to physical pixels:
        let clip_min_x = pixels_per_point * clip_rect.min.x;
        let clip_min_y = pixels_per_point * clip_rect.min.y;
        let clip_max_x = pixels_per_point * clip_rect.max.x;
        let clip_max_y = pixels_per_point * clip_rect.max.y;

        // Round to integer:
        let clip_min_x = clip_min_x.round() as i32;
        let clip_min_y = clip_min_y.round() as i32;
        let clip_max_x = clip_max_x.round() as i32;
        let clip_max_y = clip_max_y.round() as i32;

        // Clamp:
        let clip_min_x = clip_min_x.clamp(0, width_px as i32);
        let clip_min_y = clip_min_y.clamp(0, height_px as i32);
        let clip_max_x = clip_max_x.clamp(clip_min_x, width_px as i32);
        let clip_max_y = clip_max_y.clamp(clip_min_y, height_px as i32);

        Self::new(
            inner,
            clip_min_x as usize,
            (height_px as i32 - clip_max_y).max(0) as usize,
            (clip_max_x - clip_min_x).max(0) as usize,
            (clip_max_y - clip_min_y).max(0) as usize,
        )
    }
}

impl<T, const N: usize> Texture<N> for Scissor<T>
where
    T: Texture<N>,
{
    type Index = T::Index;
    type Texel = T::Texel;

    fn size(&self) -> [Self::Index; N] {
        self.inner.size()
    }

    fn read(&self, index: [Self::Index; N]) -> Self::Texel {
        self.inner.read(index)
    }
}

impl<T: Target> Target for Scissor<T> {
    unsafe fn read_exclusive_unchecked(&self, x: usize, y: usize) -> Self::Texel {
        unsafe { self.inner.read_exclusive_unchecked(x, y) }
    }

    unsafe fn write_exclusive_unchecked(&self, x: usize, y: usize, texel: Self::Texel) {
        if self.bounds_check(x, y) {
            unsafe {
                self.inner.write_exclusive_unchecked(x, y, texel);
            }
        }
    }
}

struct SoftwareTexture {
    pixels: euc::Buffer2d<egui::Rgba>,
    options: egui::TextureOptions,
}

pub struct Painter {
    textures: HashMap<TextureId, SoftwareTexture>,
}

impl Painter {
    pub fn paint_and_update_textures(
        &mut self,
        textures_delta: &TexturesDelta,
        clipped_primitives: &[ClippedPrimitive],
        pixels_per_point: f32,
        screen_size: [usize; 2],
    ) -> euc::Buffer2d<u32> {
        self.allocate_textures(textures_delta);

        let image = self.render(clipped_primitives, pixels_per_point, screen_size);

        self.free_textures(textures_delta);

        image
    }

    fn allocate_textures(&mut self, textures_delta: &TexturesDelta) {
        for (id, delta) in &textures_delta.set {
            if let Some(texture) = self.textures.get_mut(id) {
                texture.update(delta);
            } else {
                if !delta.is_whole() {
                    self.textures.insert(
                        id.clone(),
                        SoftwareTexture::new(delta.image.clone(), delta.options),
                    );
                } else {
                    panic!("Attempted partial update on absent texture")
                }
            }
        }
    }

    fn free_textures(&mut self, textures_delta: &TexturesDelta) {
        for id in &textures_delta.free {
            self.textures.remove(id);
        }
    }

    fn render(
        &mut self,
        clipped_primitives: &[ClippedPrimitive],
        pixels_per_point: f32,
        screen_size: [usize; 2],
    ) -> Buffer2d<u32> {
        let mut color = Buffer2d::fill(screen_size, 0);
        let mut depth = Buffer2d::fill(screen_size, 1.0);

        for item in clipped_primitives {
            if let epaint::Primitive::Mesh(mesh) = &item.primitive {
                self.render_mesh(
                    mesh,
                    pixels_per_point,
                    &mut color,
                    &mut depth,
                    item.clip_rect,
                );
            }
        }

        color
    }

    fn render_mesh(
        &mut self,
        mesh: &egui::Mesh,
        pixels_per_point: f32,
        color: &mut Buffer2d<u32>,
        depth: &mut Buffer2d<f32>,
        clip_rect: egui::Rect,
    ) {
        //let mut scissor = Scissor::new(&mut color, 100, 100, 100, 100);
        let texture = self
            .textures
            .get(&mesh.texture_id)
            .expect("Mesh referenced absent texture");

        let pipeline = EguiMeshEucPipeline {
            vertices: &mesh.vertices,
            sampler: texture.sampler(),
        };

        pipeline.render(
            //indices,
            &mesh.indices,
            color,
            //&mut scissor,
            depth,
        );
    }
}

impl SoftwareTexture {
    pub fn new(image: epaint::ImageData, options: TextureOptions) -> Self {
        let pixels = Buffer2d::fill([image.width(), image.height()], Rgba::RED);

        let delta = epaint::ImageDelta::full(image, options);

        let mut inst = Self { pixels, options };

        inst.update(&delta);

        inst
    }

    pub fn update(&mut self, delta: &epaint::ImageDelta) {
        let epaint::ImageData::Color(patch) = &delta.image;

        if delta.is_whole() && patch.size != self.pixels.size() {
            *self = Self::new(delta.image.clone(), delta.options);
            return;
        }

        let [off_x, off_y] = delta.pos.unwrap_or([0, 0]);

        for y in 0..delta.image.width() {
            for x in 0..delta.image.height() {
                self.pixels
                    .write(x + off_x, y + off_y, patch[(x, y)].into());
            }
        }
    }

    pub fn sampler<'a>(
        &'a self,
    ) -> Box<
        dyn Sampler<2, Index = f32, Sample = egui::Rgba, Texture = Buffer2d<Rgba>>
            + Send
            + Sync
            + 'a,
    > {
        todo!()
    }
}
