// This program is free software: you can redistribute it and/or modify
// it under the terms of the Lesser GNU General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// Lesser GNU General Public License for more details.

// You should have received a copy of the Lesser GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

// Copyright 2019 E-Nguyen Developers.

use crate::application::SettingsLauncher;
use crate::errors::{FrameError, VulkanoError};
use crate::ewin::{GpuPicker, SwapWindow};
use crate::input;
use crate::input::{KeyTracker, MouseTracker, UserEvent};
use crate::rendering::{
    diag_grad_fsm, diag_grad_vsm, uv_image_fsm, uv_image_vsm, FrameState, Framer, XyUvVertex,
    XyVertex,
};

use image;
use image::ImageFormat;
use log::error;
use rusttype::{point, Font, Scale};
use std::error::Error;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet};
use vulkano::format::Format;
use vulkano::framebuffer::{FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{Dimensions, ImmutableImage};
use vulkano::pipeline::blend::AttachmentBlend;
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};
use vulkano::sync;
use vulkano::sync::{FlushError, GpuFuture};
use vulkano_glyph::{GlyphBrush, Section};
use vulkano_win::VkSurfaceBuild;
use winit;
use winit::dpi::LogicalSize;
use winit::Icon;

pub static HEIGHT: u32 = 600;
pub static WIDTH: u32 = 370;
pub static LOGO_WIDTH: u32 = 201;
pub static LOGO_HEIGHT: u32 = 121;

pub fn settings_ui(launcher: &SettingsLauncher) -> Result<(), VulkanoError> {
    // TODO pass picker in
    let picker = GpuPicker::new().unwrap();

    let icon_data = include_bytes!("../logo/icon.png");

    let mut events_loop = winit::EventsLoop::new();
    let ldim = LogicalSize::from((WIDTH, HEIGHT));
    let surface = winit::WindowBuilder::new()
        .with_dimensions(ldim)
        .with_resizable(false)
        .with_window_icon(Icon::from_bytes(icon_data).ok())
        .with_title("E-Nguyen Settings")
        .build_vk_surface(&events_loop, picker.instance.clone())
        .unwrap();

    let mut swap_win = SwapWindow::new(&picker, &surface)?;
    let resources = SettingsResources::new()?;
    let (mut framer, mut frame_state): (SettingsFramer, SettingsState) =
        SettingsFramer::new(&mut swap_win, &resources)?;

    let mut mt = MouseTracker::new();
    let mut kt = KeyTracker::new();
    let mut done = false;
    loop {
        let result = framer.render_one(&mut swap_win, frame_state, &resources);
        match result {
            Ok(new_state) => {
                frame_state = new_state;
            }
            Err(_e) => {
                frame_state = SettingsState {
                    recreate_swapchain: true,
                    previous_frame: Box::new(vulkano::sync::now(swap_win.device.clone())),
                };
            }
        }

        events_loop.poll_events(|ev| {
            match &ev {
                winit::Event::WindowEvent { event: winit::WindowEvent::CloseRequested, .. } => {
                    done = true
                }
                winit::Event::WindowEvent { event: winit::WindowEvent::Resized(_), .. } => {
                    frame_state.recreate_swapchain = true;
                }
                _ => {}
            }

            if let Some(pe) = input::process(&ev) {
                if let Some(ue) = kt.update(&pe) {
                    match &ue {
                        UserEvent::KeyPress { character: c } => {
                            match &c {
                                '\u{1b}' => {
                                    // escape key
                                    done = true;
                                }
                                'm' => {
                                    launcher.launch_mez();
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(_ue) = mt.update(&pe) {
                    // info!("Mousetracker: {:#?}", ue);
                }
            }
        });
        if done {
            break;
        }
    }
    Ok(())
}

pub struct SettingsResources<'s> {
    font: Font<'s>,
}

impl<'s> SettingsResources<'s> {
    fn new() -> Result<SettingsResources<'s>, VulkanoError> {
        match Font::from_bytes(include_bytes!("../font/MajorMonoDisplay-Regular.ttf") as &[u8]) {
            Ok(font) => Ok(SettingsResources { font }),
            Err(err) => Err(VulkanoError::from("Font loading failed")),
        }
    }
}

pub struct SettingsFramer<'f> {
    framebuffers: Vec<Arc<FramebufferAbstract + Send + Sync>>,
    render_pass: Arc<RenderPassAbstract + Send + Sync>,
    pipeline: Arc<GraphicsPipelineAbstract + Send + Sync>,
    background_pipe: Arc<GraphicsPipelineAbstract + Send + Sync>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[XyUvVertex]>>,
    background_rect: Arc<CpuAccessibleBuffer<[XyVertex]>>,
    set: Arc<dyn DescriptorSet + Send + Sync>,
    title: Vec<Section>,
    glyph_brush: GlyphBrush<'f>,
}

impl<'f, 'r: 'f> Framer<'f, 'r, SettingsFramer<'f>, SettingsState, SettingsResources<'r>>
    for SettingsFramer<'f>
{
    fn new(
        swap_win: &mut SwapWindow,
        resources: &'f SettingsResources<'r>,
    ) -> Result<(SettingsFramer<'f>, SettingsState), VulkanoError> {
        let background_rect = {
            CpuAccessibleBuffer::from_iter(
                swap_win.device.clone(),
                BufferUsage::all(),
                [
                    XyVertex { position: [-1.0, -1.0] },
                    XyVertex { position: [1.0, -1.0] },
                    XyVertex { position: [-1.0, 1.0] },
                    XyVertex { position: [1.0, 1.0] },
                ]
                .iter()
                .cloned(),
            )?
        };

        let vertex_buffer: Arc<CpuAccessibleBuffer<[XyUvVertex]>> = {
            // TODO move this resize-dependent logic to allow both swapchain and
            // Framers to adjust to the new window dimensions
            let window = swap_win.surface.window();
            let dimensions = if let Some(dimensions) = window.get_inner_size() {
                let dimensions: (u32, u32) =
                    dimensions.to_physical(window.get_hidpi_factor()).into();
                [dimensions.0, dimensions.1]
            } else {
                [WIDTH, HEIGHT]
            };

            let dims = [dimensions[0] as f32, dimensions[1] as f32];

            let top_pad: f32 = 24.0;
            let left_pad: f32 = (dims[0] - (LOGO_WIDTH as f32)) / 2.0;
            let x0: f32 = -1.0 + left_pad / dims[0] * 2.0;
            let x1: f32 = -x0;
            let y0: f32 = -1.0 + top_pad / (dims[1]) * 2.0;
            let y1: f32 = y0 + (LOGO_HEIGHT as f32) / dims[1] * 2.0;

            CpuAccessibleBuffer::from_iter(
                swap_win.device.clone(),
                BufferUsage::all(),
                [
                    XyUvVertex { position: [x0, y0], uv: [0.0, 0.0] },
                    XyUvVertex { position: [x1, y0], uv: [1.0, 0.0] },
                    XyUvVertex { position: [x0, y1], uv: [0.0, 1.0] },
                    XyUvVertex { position: [x1, y1], uv: [1.0, 1.0] },
                ]
                .iter()
                .cloned(),
            )
            .unwrap()
        };

        let vs = uv_image_vsm::Shader::load(swap_win.device.clone()).unwrap();
        let fs = uv_image_fsm::Shader::load(swap_win.device.clone()).unwrap();

        let render_pass = Arc::new(
            vulkano::single_pass_renderpass!(swap_win.device.clone(),
                                             attachments: {
                                                 color: {
                                                     load: Clear,
                                                     store: Store,
                                                     format: swap_win.swapchain.format(),
                                                     samples: 1,
                                                 }
                                             },

                                             pass: {
                                                 color: [color],
                                                 depth_stencil: {}
                                             }
            )
            .unwrap(),
        );

        let pipeline: Arc<GraphicsPipelineAbstract + Send + Sync> = Arc::new(
            GraphicsPipeline::start()
                .blend_collective(AttachmentBlend::alpha_blending())
                .vertex_input_single_buffer::<XyUvVertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .triangle_strip()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                .build(swap_win.device.clone())
                .unwrap(),
        );
        let framebuffers = swap_win.size_dependent_setup(render_pass.clone())?;

        let vs_grad = diag_grad_vsm::Shader::load(swap_win.device.clone()).unwrap();
        let fs_grad = diag_grad_fsm::Shader::load(swap_win.device.clone()).unwrap();

        let background_pipe = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<XyVertex>()
                .vertex_shader(vs_grad.main_entry_point(), ())
                .triangle_strip()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs_grad.main_entry_point(), ())
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                .build(swap_win.device.clone())
                .unwrap(),
        );

        // texture kept alive by descriptor set
        let (texture, tex_future) = {
            let image = image::load_from_memory_with_format(
                include_bytes!("../logo/eye_of_nguyen_settings_logo.png"),
                ImageFormat::PNG,
            )
            .unwrap()
            .to_rgba();
            let image_data = image.into_raw().clone();

            ImmutableImage::from_iter(
                image_data.iter().cloned(),
                Dimensions::Dim2d { width: LOGO_WIDTH, height: LOGO_HEIGHT },
                Format::R8G8B8A8Srgb,
                swap_win.window_queue.clone(),
            )
            .unwrap()
        };

        let texture_future = Box::new(tex_future);

        // lives in descriptor set
        let sampler = Sampler::new(
            swap_win.device.clone(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::Repeat,
            0.0,
            1.0,
            0.0,
            0.0,
        )
        .unwrap();

        let set = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(texture.clone(), sampler.clone())
                .unwrap()
                .build()
                .unwrap(),
        );

        let subpass =
            Subpass::from(render_pass.clone() as Arc<RenderPassAbstract + Send + Sync>, 0)
                .ok_or("Subpass is None")?;
        let mut glyph_brush = GlyphBrush::new(&swap_win.device, subpass.clone()).unwrap();

        let title = vec![
            (glyph_brush.queue_glyphs(
                resources.font.layout("E-NGUYEN", Scale::uniform(72.0), point(56.0, 256.0)),
                0,
                [1.0, 1.0, 1.0, 1.0],
            )),
        ];

        let copy_future =
            glyph_brush.cache_sections(&swap_win.window_queue, title.iter()).unwrap().unwrap();

        let texture_future: Box<dyn GpuFuture> = Box::new(texture_future.join(copy_future));

        let settings_framer = SettingsFramer {
            render_pass,
            pipeline,
            set,
            framebuffers,
            vertex_buffer,
            background_rect,
            background_pipe,
            title,
            glyph_brush,
        };
        let frame_state =
            SettingsState { previous_frame: texture_future, recreate_swapchain: false };
        Ok((settings_framer, frame_state))
    }

    fn render_one(
        &mut self,
        swap_win: &mut SwapWindow,
        mut frame_state: SettingsState,
        resources: &SettingsResources,
    ) -> Result<SettingsState, VulkanoError> {
        // TODO memory swaps = lifetime impedence
        let mut previous_frame = Box::new(sync::now(swap_win.device.clone())) as Box<GpuFuture>;
        std::mem::swap(&mut previous_frame, &mut frame_state.previous_frame);
        previous_frame.cleanup_finished();

        if frame_state.recreate_swapchain {
            self.framebuffers = swap_win.recreate_swapchain(self.render_pass.clone())?;
        }

        let (image_num, acquire_future) = swap_win.future_image()?;

        let clear_values = vec![[0.0, 0.0, 0.0, 1.0].into()];

        let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(
            swap_win.device.clone(),
            swap_win.window_queue.family(),
        )
        .unwrap()
        .begin_render_pass(self.framebuffers[image_num].clone(), false, clear_values)
        .unwrap()
        .draw(
            self.background_pipe.clone(),
            &swap_win.dynamic_state,
            vec![self.background_rect.clone()],
            (),
            (),
        )
        .unwrap()
        .draw(
            self.pipeline.clone(),
            &swap_win.dynamic_state,
            vec![self.vertex_buffer.clone()],
            self.set.clone(),
            (),
        )
        .unwrap();
        let command_buffer = self
            .glyph_brush
            .draw(
                command_buffer,
                &self.title,
                &swap_win.dynamic_state,
                [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                ],
                swap_win.f_dimensions().unwrap(),
            )
            .unwrap()
            .end_render_pass()
            .unwrap()
            .build()
            .unwrap();

        let new_frame = previous_frame
            .join(acquire_future)
            .then_execute(swap_win.window_queue.clone(), command_buffer)
            .unwrap()
            .then_swapchain_present(
                swap_win.window_queue.clone(),
                swap_win.swapchain.clone(),
                image_num,
            )
            .then_signal_fence_and_flush();

        match new_frame {
            Ok(frame) => {
                frame_state.previous_frame = Box::new(frame);
                Ok(frame_state)
            }
            // TODO research which of these are recoverable
            Err(e @ FlushError::OutOfDate) => Err((Box::new(e) as Box<Error>).into()),
            Err(e) => Err((Box::new(e) as Box<Error>).into()),
        }
    }
}

struct SettingsState {
    pub previous_frame: Box<GpuFuture>,
    pub recreate_swapchain: bool,
}

impl FrameState for SettingsState {}
