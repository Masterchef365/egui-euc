#![no_std]

use egui::{
    epaint, ClippedPrimitive, Color32, Rgba, TextureFilter, TextureId, TextureOptions,
    TextureWrapMode, TexturesDelta,
};
use euc::{Buffer2d, CullMode, Pipeline, Sampler, Target, Texture, TriangleList};
use hashbrown::HashMap;
use num_traits::Float;

/// Egui vertex data which is algebraic (has Mul and Add)
#[derive(Clone, Copy, Debug)]
pub struct EguiVertexData {
    pub uv: egui::Pos2,
    pub color: egui::Rgba,
}

impl core::ops::Mul<f32> for EguiVertexData {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            uv: self.uv.to_vec2().mul(rhs).to_pos2(),
            color: self.color.mul(rhs),
        }
    }
}

impl core::ops::Add<Self> for EguiVertexData {
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

/// Euc Pipeline which can draw an egui mesh, using `sampler` as a texture.
pub struct EguiMeshEucPipeline<'r, S> {
    pub sampler: S,
    pub vertices: &'r [epaint::Vertex],
    pub screen_size_points: egui::Vec2,
}

pub fn egui_coord_to_ndc(pos: egui::Pos2, screen_size: egui::Vec2) -> [f32; 2] {
    let transf = 2.0 * pos.to_vec2() / screen_size;
    [transf.x - 1.0, 1.0 - transf.y]
}

impl<'r, S> Pipeline<'r> for EguiMeshEucPipeline<'r, S>
where
S: Sampler<2, Index = f32, Sample = egui::Rgba>,
{
    type Vertex = u32;
    type VertexData = EguiVertexData;
    type Primitives = TriangleList;
    type Pixel = u32;
    type Fragment = Rgba;

    #[inline(always)]
    fn vertex(&self, idx: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        let vertex = self.vertices[*idx as usize];
        let [x, y] = egui_coord_to_ndc(vertex.pos, self.screen_size_points);
        let xyzw = [x, y, 0.0, 1.0];
        (xyzw, vertex.into())
    }

    #[inline(always)]
    fn fragment(&self, color: Self::VertexData) -> Self::Fragment {
        color.color * self.sampler.sample([color.uv.x, color.uv.y])
    }

    fn blend(&self, screen: Self::Pixel, fragment: Self::Fragment) -> Self::Pixel {
        let [r, g, b, a] = screen.to_le_bytes();
        let screen = Color32::from_rgba_premultiplied(r, g, b, a);
        let screen: Rgba = screen.into();

        let mut color = fragment + screen * (1.0 - fragment.a());
        color[3] = screen.a() + fragment.a() * (1.0 - screen.a());

        u32::from_le_bytes(color.to_srgba_unmultiplied())
    }

    fn rasterizer_config(&self) -> CullMode {
        CullMode::None
    }
}

/// Wrapper of a euc::Target, reads are unaffected but writes are clipped
/// by the given rectangle. 
pub struct Scissor<T> {
    pub inner: T,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
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
        [width_px, height_px]: [usize; 2],
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
            clip_min_y as usize,
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

/// A persistent object which tracks textures and can render an image from clipped primitives.
pub struct Painter {
    textures: HashMap<TextureId, SoftwareTexture>,
}

impl Painter {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

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
                if delta.is_whole() {
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
                let mut scissor = Scissor::from_clip_rect(
                    &mut color,
                    screen_size,
                    pixels_per_point,
                    item.clip_rect,
                );

                let texture = self
                    .textures
                    .get(&mesh.texture_id)
                    .expect("Mesh referenced absent texture");

                let pixels = &texture.pixels;

                let screen_size_points = egui::Vec2::new(screen_size[0] as f32, screen_size[1] as f32) / pixels_per_point;

                // TODO: This dumb as HELL
                match (texture.options.magnification, texture.options.wrap_mode) {
                    (TextureFilter::Linear, TextureWrapMode::Repeat) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.linear().tiled(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                    (TextureFilter::Linear, TextureWrapMode::ClampToEdge) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.linear().clamped(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                    (TextureFilter::Linear, TextureWrapMode::MirroredRepeat) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.linear().mirrored(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                    (TextureFilter::Nearest, TextureWrapMode::Repeat) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.nearest().tiled(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                    (TextureFilter::Nearest, TextureWrapMode::ClampToEdge) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.nearest().clamped(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                    (TextureFilter::Nearest, TextureWrapMode::MirroredRepeat) => {
                        EguiMeshEucPipeline {
                            vertices: &mesh.vertices,
                            sampler: pixels.nearest().mirrored(),
                            screen_size_points,
                        }
                        .render(&mesh.indices, &mut scissor, &mut depth);
                    }
                };
            }
        }

        color
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

        self.options = delta.options;

        let [off_x, off_y] = delta.pos.unwrap_or([0, 0]);

        for y in 0..delta.image.height() {
            for x in 0..delta.image.width() {
                let sample = patch[(x, y)];
                self.pixels
                    .write(x + off_x, y + off_y, sample.into());
            }
        }
    }
}

pub fn euc_to_egui_colorimage(euc: euc::Buffer2d<u32>) -> egui::ColorImage {
    let pixels = euc.raw().iter().map(|px| {
        let [r, g, b, a] = px.to_le_bytes();
        egui::Color32::from_rgba_unmultiplied(r, g, b, a)
    })
    .collect();
    egui::ColorImage::new(euc.size(), pixels)
}

/// Helper to provide an image given successive egui::RawInputs
pub struct SoftwareGui {
    pub egui_ctx: egui::Context,
    pub software_render: Painter,
}

impl SoftwareGui {
    pub fn new() -> Self {
        Self {
            egui_ctx: Default::default(),
            software_render: Painter::new(),
        }
    }

    pub fn update(
        &mut self,
        new_input: egui::RawInput,
        screen_size: [usize; 2],
        sub_gui: impl FnMut(&egui::Context),
    ) -> egui::ColorImage {

        let output = self.egui_ctx.run(new_input, sub_gui);
        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let clipped_primitives = self.egui_ctx.tessellate(output.shapes, pixels_per_point);
        let buffer = self.software_render.paint_and_update_textures(
            &output.textures_delta,
            &clipped_primitives,
            pixels_per_point,
            screen_size,
        );
        euc_to_egui_colorimage(buffer)
    }
}


