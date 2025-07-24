use egui::{pos2, Vec2};
use egui_euc::euc_to_egui_colorimage;

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

fn main() -> anyhow::Result<()> {
    eframe::run_native(
        "eframe template",
        Default::default(),
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
    .map_err(|e| anyhow::format_err!("{e}"))
}

struct App {
    tex: egui::TextureId,
    sub: SubGui,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let options = egui::TextureOptions::NEAREST;
        let image = egui::ImageData::from(egui::ColorImage::filled(
            [WIDTH, HEIGHT],
            egui::Color32::RED,
        ));
        let tex = cc
            .egui_ctx
            .tex_manager()
            .write()
            .alloc("sub-gui".into(), image, options);

        Self {
            tex,
            sub: SubGui::new(),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("hi");

            let (resp, painter) = ui.allocate_painter(
                Vec2::new(WIDTH as _, HEIGHT as _),
                egui::Sense::click_and_drag(),
            );
            painter.image(
                self.tex,
                resp.rect,
                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );

            let raw_input = ctx.input(|r| r.raw.clone());
            let new_image = self.sub.update(raw_input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.strong("STRONG aura");
                });
            });

            ui.ctx().tex_manager().write().set(
                self.tex,
                egui::epaint::ImageDelta::full(new_image, egui::TextureOptions::NEAREST),
            );
        });
    }
}

struct SubGui {
    egui_ctx: egui::Context,
    software_render: egui_euc::Painter,
}

impl SubGui {
    pub fn new() -> Self {
        Self {
            egui_ctx: Default::default(),
            software_render: egui_euc::Painter::new(),
        }
    }

    pub fn update(
        &mut self,
        new_input: egui::RawInput,
        sub_gui: impl FnMut(&egui::Context),
    ) -> egui::ColorImage {
        let output = self.egui_ctx.run(new_input, sub_gui);
        let pixels_per_point = self.egui_ctx.pixels_per_point();
        let clipped_primitives = self.egui_ctx.tessellate(output.shapes, pixels_per_point);
        let buffer = self.software_render.paint_and_update_textures(
            &output.textures_delta,
            &clipped_primitives,
            pixels_per_point,
            [WIDTH, HEIGHT],
        );
        euc_to_egui_colorimage(buffer)
    }
}
