#![no_std]

use egui::{
    ClippedPrimitive, Color32, ImageSource, Rgba, TextureFilter, TextureId, TextureOptions, TextureWrapMode, TexturesDelta, epaint
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
    type Pixel = Algebra565;
    type Fragment = Rgba;

    #[inline(always)]
    fn vertex(&self, idx: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        let vertex = self.vertices[*idx as usize];
        let [x, y] = egui_coord_to_ndc(vertex.pos, self.screen_size_points);
        let xyzw = [x, y, 0.0, 1.0];
        (xyzw, vertex.into())
    }

    #[inline(always)]
    fn fragment(&self, vd: Self::VertexData) -> Self::Fragment {
        vd.color * self.sampler.sample([vd.uv.x, vd.uv.y])
    }

    fn blend(&self, screen: Self::Pixel, fragment: Self::Fragment) -> Self::Pixel {
        let [b, g, r] = screen.to_bgrf();
        let screen = Rgba::from_rgb(r, g, b);

        let color = fragment + screen * (1.0 - fragment.a());

        Algebra565::from_bgrf([color.b(), color.g(), color.r()])
    }

    fn rasterizer_config(&self) -> CullMode {
        CullMode::None
    }
}

/// Presents inner as a larger texture with dimensions screen_width and screen_height
pub struct Viewport<T> {
    pub inner: T,
    pub x: usize,
    pub y: usize,
    pub screen_width: usize,
    pub screen_height: usize,
}

impl<T: Texture<2, Index = usize>> Viewport<T> {
    pub fn new(inner: T, x: usize, y: usize, screen_width: usize, screen_height: usize) -> Self {
        Self {
            inner,
            x,
            y,
            screen_width,
            screen_height,
        }
    }

    fn bounds_check(&self, x: usize, y: usize) -> bool {
        let [w, h] = self.inner.size();
        x >= self.x && y >= self.y && x < self.x + w && y < self.y + h
    }
}


impl<T> Texture<2> for Viewport<T>
where
    T: Texture<2, Index = usize>,
{
    type Index = usize;
    type Texel = T::Texel;

    fn size(&self) -> [Self::Index; 2] {
        [self.screen_width, self.screen_height]
    }

    fn read(&self, index: [Self::Index; 2]) -> Self::Texel {
        let [x, y] = index;
        if self.bounds_check(x, y) {
            self.inner.read([x - self.x, y - self.y])
        } else {
            self.inner.read([self.x, self.y])
        }
    }
}

impl<T: Target> Target for Viewport<T> {
    unsafe fn read_exclusive_unchecked(&self, x: usize, y: usize) -> Self::Texel {
        if self.bounds_check(x, y) {
            unsafe { self.inner.read_exclusive_unchecked(x - self.x, y - self.y) }
        } else {
            unsafe { self.inner.read_exclusive_unchecked(self.x, self.y) }
        }
    }

    unsafe fn write_exclusive_unchecked(&self, x: usize, y: usize, texel: Self::Texel) {
        if self.bounds_check(x, y) {
            unsafe {
                self.inner.write_exclusive_unchecked(x - self.x, y - self.y, texel);
            }
        }
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

struct ColorImageTexture(egui::ColorImage);

impl Texture<2> for ColorImageTexture {
    type Index = usize;

    type Texel = egui::Rgba;

    #[inline]
    fn size(&self) -> [Self::Index; 2] {
        [self.0.width(), self.0.height()]
    }

    #[inline]
    fn read(&self, index: [Self::Index; 2]) -> Self::Texel {
        self.0[(index[0], index[1])].into()
    }

    #[inline(always)]
    unsafe fn read_unchecked(&self, index: [Self::Index; 2]) -> Self::Texel {
        // TODO: unchecked
        self.0[(index[0], index[1])].into()
    }

}


struct SoftwareTexture {
    pixels: ColorImageTexture,
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
        mut textures_delta: TexturesDelta,
        clipped_primitives: &[ClippedPrimitive],
        pixels_per_point: f32,
        screen_size: [usize; 2],
        tile_size: usize,
        mut draw_colors: impl FnMut(usize, usize, usize, usize, &Buffer2d<Algebra565>),
    ) {
        self.allocate_textures(&mut textures_delta);

        let mut color: Buffer2d<Algebra565> = Buffer2d::fill([tile_size; 2], Algebra565::BLACK);

        for x in (0..screen_size[0]).step_by(tile_size) {
            for y in (0..screen_size[1]).step_by(tile_size) {
                let [ex, ey] = self.render(clipped_primitives, pixels_per_point, screen_size, [x, y], &mut color);
                draw_colors(x, y, ex, ey, &color);
            }
        }

        drop(color);

        self.free_textures(&mut textures_delta);
    }

    fn allocate_textures(&mut self, textures_delta: &mut TexturesDelta) {
        for (id, delta) in textures_delta.set.drain(..) {
            if let Some(texture) = self.textures.get_mut(&id) {
                texture.update(&delta);
            } else {
                if delta.is_whole() {
                    self.textures.insert(
                        id.clone(),
                        SoftwareTexture::new(delta.image, delta.options),
                    );
                } else {
                    panic!("Attempted partial update on absent texture")
                }
            }
        }
    }

    fn free_textures(&mut self, textures_delta: &mut TexturesDelta) {
        for id in textures_delta.free.drain(..) {
            self.textures.remove(&id);
        }
    }

    fn render(
        &mut self,
        clipped_primitives: &[ClippedPrimitive],
        pixels_per_point: f32,
        screen_size: [usize; 2],
        buf_offset: [usize; 2],
        color: &mut Buffer2d<Algebra565>,
    ) -> [usize; 2] {
        let mut depth = Buffer2d::fill([1,1], 1.0);

        let clip_min = buf_offset;
        let buf_size = color.size(); 
        let mut clip_max = [0, 0];
        for i in 0..2 {
            clip_max[i] = (clip_min[i] + buf_size[i]).min(screen_size[i]);
        }

        for item in clipped_primitives {
            if let epaint::Primitive::Mesh(mesh) = &item.primitive {
                let viewport = Viewport::new(&mut *color, buf_offset[0], buf_offset[1], screen_size[0], screen_size[1]);

                let mut scissor = Scissor::from_clip_rect(
                    viewport,
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

        clip_max
    }
}

impl SoftwareTexture {
    pub fn new(image: epaint::ImageData, options: TextureOptions) -> Self {
        let epaint::ImageData::Color(data) = &image;

        let inst = Self { pixels: ColorImageTexture(data.as_ref().clone()), options };

        //let delta = epaint::ImageDelta::full(image, options);

        //inst.update(&delta);

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

        if let egui::epaint::image::ImageStorage::Owned(_) = &self.pixels.0.pixels {
            for y in 0..delta.image.height() {
                for x in 0..delta.image.width() {
                    let sample = patch[(x, y)];
                    let xf = x + off_x;
                    let yf = y + off_y;
                    self.pixels.0[(xf, yf)] = sample;
                }
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
        tile_size: usize,
        sub_gui: impl FnMut(&egui::Context),
        draw_colors: impl FnMut(usize, usize, usize, usize, &Buffer2d<Algebra565>),
    ) {
        let (shapes, textures_delta);
        {
            let output = self.egui_ctx.run(new_input, sub_gui);
            shapes = output.shapes;
            textures_delta = output.textures_delta;
        }

        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);

        self.software_render.paint_and_update_textures(
            textures_delta,
            &clipped_primitives,
            pixels_per_point,
            screen_size,
            tile_size,
            draw_colors,
        )
    }
}


#[derive(Copy, Clone, Default)]
pub struct Algebra565 {
    pub bits: u16,
}

fn float_to_bits(value: f32, nbits: u8) -> u16 {
    let maxval = ((1u16 << nbits) - 1) as f32;
    (value.clamp(0.0, 1.0) * maxval).floor() as u16
}

fn bits_to_float(bits: u16, nbits: u8) -> f32 {
    let maxval = ((1u16 << nbits) - 1) as f32;
    bits as f32 / maxval
}

fn extract_bits_range(bits: u16, nbits: u8, position: u8) -> u16 {
    (bits >> position) & ((1 << nbits) - 1)
}

impl Algebra565 {
    pub const BLACK: Self = Self { bits: 0b1111100000000000 };
    pub const RED: Self = Self { bits: 0b1111100000000000 };
    pub const GREEN: Self = Self { bits: 0b0000011111100000 };
    pub const BLUE: Self = Self { bits: 0b0000000000011111 };
    pub const CYAN: Self = Self { bits: 0b0000011111111111 };
    pub const YELLOW: Self = Self { bits: 0b1111111111000000 };
    pub const MAGENTA: Self = Self { bits: 0b1111100000011111 };

    pub fn new(bits: u16) -> Self {
        Self { bits }
    }

    pub fn to_bgrf(&self) -> [f32; 3] {
        [
            bits_to_float(extract_bits_range(self.bits, 5, 0), 5),
            bits_to_float(extract_bits_range(self.bits, 6, 5), 6),
            bits_to_float(extract_bits_range(self.bits, 5, 6+5), 5),
        ]
    }

    pub fn from_bgrf([b, g, r]: [f32; 3]) -> Self {
        let mut bits = 0;
        bits |= float_to_bits(b, 5);
        bits |= float_to_bits(g, 6) << 5;
        bits |= float_to_bits(r, 5) << (5+6);
        Self { bits }
    }
}

#[cfg(test)]
#[test]
fn test_algebra565_roundtrip() {
    assert_eq!(Algebra565::from_bgrf([1., 0., 0.]).to_bgrf(), [1.0, 0.0, 0.0]);
    assert_eq!(Algebra565::from_bgrf([0., 1., 0.]).to_bgrf(), [0.0, 1.0, 0.0]);
    assert_eq!(Algebra565::from_bgrf([0., 0., 1.]).to_bgrf(), [0.0, 0.0, 1.0]);

    assert_eq!(Algebra565::from_bgrf([15.0/31.0, 19.0/63.0, 19.0/31.0]).to_bgrf(), [15.0/31.0, 19.0/63.0, 19.0/31.0]);
}

impl euc::math::WeightedSum for Algebra565 {
    fn weighted_sum<const N: usize>(
        values: [Self; N],
        weights: [f32; N],
    ) -> Self {
        let mut sum = [0_f32; 3];

        for i in 0..N {
            let bgr = values[i].to_bgrf();

            for j in 0..3 {
                sum[j] += bgr[j] * weights[i];
            }
        }

        Self::from_bgrf(sum)
    }
}
