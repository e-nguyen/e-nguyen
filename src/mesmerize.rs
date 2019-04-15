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

use crate::application::MezLauncher;
use crate::compute::{AudioTex, AudioTexSource, AudioTexTap};
use crate::errors::VulkanoError;
use crate::ewin::{GpuPicker, SwapWindow};
use crate::input;
use crate::input::{KeyTracker, MouseTracker, UserEvent};
use crate::rendering::{uv_image_vsm, uv_scroll_fsm, FrameState, Framer, XyUvVertex};

use log::error;
use std::error::Error;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor::descriptor_set::{DescriptorSet, PersistentDescriptorSet};
use vulkano::format::Format;
use vulkano::framebuffer::{FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{Dimensions, StorageImage};
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};
use vulkano::sync::FlushError;
use vulkano::sync::GpuFuture;
use vulkano_win::VkSurfaceBuild;
use winit;
use winit::Icon;

pub fn mezmerize(launcher: &MezLauncher) -> Result<(), VulkanoError> {
    let picker = launcher.picker.clone();

    let icon_data = include_bytes!("../logo/icon.png");

    let mut events_loop = winit::EventsLoop::new();
    let surface = winit::WindowBuilder::new()
        .with_window_icon(Icon::from_bytes(icon_data).ok())
        .with_title("E-Nguyen")
        .build_vk_surface(&events_loop, picker.instance.clone())
        .unwrap();

    let mut swap_window = SwapWindow::new(&picker, &surface)?;
    let mut _r = MezResources {};
    let (mut framer, mut frame_state): (MezFramer, MezState) =
        MezFramer::new(&mut swap_window, &_r)?;

    let mut mt = MouseTracker::new();
    let mut kt = KeyTracker::new();
    let mut done = false;

    loop {
        let result = framer.render_one(&mut swap_window, frame_state, &_r);
        match result {
            Ok(new_state) => {
                frame_state = new_state;
            }
            Err(_e) => {
                frame_state = MezState {
                    recreate_swapchain: true,
                    previous_frame: Box::new(vulkano::sync::now(swap_window.device.clone())),
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
                    println!("key in mez");
                    dbg!(&ev);
                    match &ue {
                        UserEvent::KeyPress { character: c } => {
                            match &c {
                                'f' => {
                                    // TODO querying window or state tracking
                                    let window = surface.window();
                                    window.set_maximized(true);
                                }
                                's' => {
                                    launcher.launch_settings();
                                }
                                '\u{1b}' => {
                                    // escape key
                                    done = true;
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(_ue) = mt.update(&pe) {}
            }
        });
        if done {
            break;
        }
    }
    Ok(())
}

struct MezResources {}

struct MezFramer {
    framebuffers: Vec<Arc<FramebufferAbstract + Send + Sync>>,
    render_pass: Arc<RenderPassAbstract + Send + Sync>,
    pipeline: Arc<GraphicsPipelineAbstract + Send + Sync>,
    fft_texture: Arc<StorageImage<Format>>,
    background_rect: Arc<CpuAccessibleBuffer<[XyUvVertex]>>,
    set: Arc<dyn DescriptorSet + Send + Sync>,
    fft_tex_index: i32,
    audio_tex_tap: AudioTexTap,
    audio_tex: Option<AudioTex>,
}

// TODO this trait bounds repeats the declaration and proceeds to use concrete
// types to build the return value.  Can it be declared parameterized on types?
impl<'a, 'f: 'a> Framer<'a, 'f, MezFramer, MezState, MezResources> for MezFramer {
    fn new(
        swap_win: &mut SwapWindow,
        _r: &MezResources,
    ) -> Result<(MezFramer, MezState), VulkanoError> {
        // creates a stream of image-futures we can use to copy to our fft_texture
        let source = AudioTexSource::new(1024).unwrap();
        let tap =
            AudioTexTap::turn_on(source, swap_win.device.clone(), swap_win.window_queue.clone())
                .unwrap();

        let vs = uv_image_vsm::Shader::load(swap_win.device.clone()).unwrap();
        let fs = uv_scroll_fsm::Shader::load(swap_win.device.clone()).unwrap();

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

        let fft_texture = StorageImage::new(
            swap_win.device.clone(),
            Dimensions::Dim2d { width: 1024, height: 1024 },
            Format::R32G32B32A32Sfloat,
            Some(swap_win.window_queue.family()),
        )
        .unwrap();

        let background_rect = {
            CpuAccessibleBuffer::from_iter(
                swap_win.device.clone(),
                BufferUsage::all(),
                [
                    XyUvVertex { position: [1.0, 1.0], uv: [0.0, 0.0] },
                    XyUvVertex { position: [-1.0, 1.0], uv: [1.0, 0.0] },
                    XyUvVertex { position: [1.0, -1.0], uv: [0.0, 1.0] },
                    XyUvVertex { position: [-1.0, -1.0], uv: [1.0, 1.0] },
                ]
                .iter()
                .cloned(),
            )?
        };

        // lives in descriptor set
        let sampler = Sampler::new(
            swap_win.device.clone(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::Repeat,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        )
        .unwrap();

        let pipeline: Arc<GraphicsPipelineAbstract + Send + Sync> = Arc::new(
            GraphicsPipeline::start()
                .triangle_strip()
                .vertex_input_single_buffer::<XyUvVertex>()
                .vertex_shader(vs.main_entry_point(), ())
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fs.main_entry_point(), ())
                .blend_alpha_blending()
                .render_pass(Subpass::from(render_pass.clone(), 0).ok_or("No subpass").unwrap())
                .build(swap_win.device.clone())
                .unwrap(),
        );

        let set = Arc::new(
            PersistentDescriptorSet::start(pipeline.clone(), 0)
                .add_sampled_image(fft_texture.clone(), sampler.clone())
                .unwrap()
                .build()
                .unwrap(),
        );

        let framebuffers = swap_win.size_dependent_setup(render_pass.clone())?;
        let framer = MezFramer {
            pipeline,
            render_pass,
            fft_texture,
            background_rect,
            framebuffers,
            set,
            audio_tex_tap: tap,
            audio_tex: None,
            fft_tex_index: 0,
        };
        let previous_frame = Box::new(vulkano::sync::now(swap_win.device.clone()));
        let frame_state = MezState { previous_frame, recreate_swapchain: false };
        Ok((framer, frame_state))
    }

    fn render_one(
        &mut self,
        swap_win: &mut SwapWindow,
        mut frame_state: MezState,
        _r: &MezResources,
    ) -> Result<MezState, VulkanoError> {
        let mut previous_frame =
            Box::new(vulkano::sync::now(swap_win.device.clone())) as Box<dyn GpuFuture>;

        // TODO memory swaps = lifetime impedence
        std::mem::swap(&mut previous_frame, &mut frame_state.previous_frame);
        previous_frame.cleanup_finished();

        if frame_state.recreate_swapchain {
            self.framebuffers = swap_win.recreate_swapchain(self.render_pass.clone())?;
        }

        let ready: Option<AudioTex> = {
            if let Some(_) = self.audio_tex {
                let mut ready = None;
                std::mem::swap(&mut ready, &mut self.audio_tex);
                ready
            } else if let Ok(fresh_tex) = self.audio_tex_tap.tap.try_recv() {
                Some(fresh_tex)
            } else {
                None
            }
        };

        let (image_num, acquire_future) = swap_win.future_image().unwrap();
        let clear_values = vec![[0.0, 0.0, 0.0, 1.0].into()];

        let mut cbb: AutoCommandBufferBuilder = AutoCommandBufferBuilder::primary_one_time_submit(
            swap_win.device.clone(),
            swap_win.window_queue.family(),
        )
        .unwrap();

        if let Some(r) = ready {
            previous_frame = Box::new(previous_frame.join(r.ready));
            previous_frame.cleanup_finished();
            let mut x: i32 = self.fft_tex_index;
            let ux: i32 = x as i32;
            cbb = cbb
                .copy_image(
                    r.buffer.clone(),
                    [0, 0, 0],
                    0,
                    0,
                    self.fft_texture.clone(),
                    [ux, 0, 0],
                    0,
                    0,
                    [1 as u32, 1024, 1],
                    1,
                )
                .unwrap();
            x += 1;
            if x + 1 > 1024 {
                x = 0;
            }
            self.fft_tex_index = x;
            self.audio_tex = None;
        }

        let push_constants =
            uv_scroll_fsm::ty::PushConstant { offset_fac: self.fft_tex_index as f32 / 1024_f32 };

        if self.audio_tex.is_none() {
            self.audio_tex = self.audio_tex_tap.tap.try_recv().ok();
        }

        cbb = cbb
            .begin_render_pass(self.framebuffers[image_num].clone(), false, clear_values)
            .unwrap()
            .draw(
                self.pipeline.clone(),
                &swap_win.dynamic_state,
                vec![self.background_rect.clone()],
                self.set.clone(),
                push_constants,
            )
            .unwrap()
            .end_render_pass()
            .unwrap();
        let cb = cbb.build().unwrap();

        let new_frame = acquire_future
            .join(previous_frame)
            .then_execute(swap_win.window_queue.clone(), cb)?
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
            Err(e) => {
                error!("{:?}", e);
                Err((Box::new(e) as Box<Error>).into())
            }
        }
    }
}

struct MezState {
    pub previous_frame: Box<dyn GpuFuture>,
    pub recreate_swapchain: bool,
}

impl FrameState for MezState {}
