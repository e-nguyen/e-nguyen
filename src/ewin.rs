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

use crate::errors::{ENguyenError, FrameError, VulkanoError};

use log::{info, warn};
use std::error::Error;
use std::sync::Arc;
use vulkano::command_buffer::DynamicState;
use vulkano::device::{Device, DeviceExtensions, Queue};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract};
use vulkano::image::SwapchainImage;
use vulkano::instance::PhysicalDeviceType;
use vulkano::instance::{Instance, PhysicalDevice, QueueFamily};
use vulkano::pipeline::viewport::Viewport;
use vulkano::swapchain;
use vulkano::swapchain::Surface;
use vulkano::swapchain::{
    AcquireError, PresentMode, SurfaceTransform, Swapchain, SwapchainCreationError,
};
use vulkano::sync::GpuFuture;
use winit;
use winit::Window;

/// A GPU chosen to draw to a surface, which owns a window, has a logical
/// device configured and at least one graphics queue.
pub struct SwapWindow {
    pub device: Arc<Device>,
    pub window_queue: Arc<Queue>,
    pub surface: Arc<Surface<Window>>,
    pub swapchain: Arc<Swapchain<Window>>,
    pub swap_images: Vec<Arc<SwapchainImage<Window>>>,
    pub dynamic_state: DynamicState,
}

impl<'a> SwapWindow {
    pub fn new(
        picker: &'a GpuPicker,
        surface: &Arc<Surface<Window>>,
    ) -> Result<SwapWindow, VulkanoError> {
        let physical = picker.discrete_or_first_device(&surface)?;
        info!("Using device: {} (type: {:?})", physical.name(), physical.ty());

        let queue_family = GpuPicker::graphics_queue_fam(&physical, &surface)
            .ok_or("Physical device has no graphics queue")?;
        let device_ext = DeviceExtensions { khr_swapchain: true, ..DeviceExtensions::none() };
        let (device, mut queues) = Device::new(
            physical,
            physical.supported_features(), // requests all supported features
            &device_ext,
            [(queue_family, 0.5)].iter().cloned(),
        )?;

        let window_queue =
            queues.next().ok_or("Logical device creation returned no supported graphics queue")?;

        let (swapchain, swap_images) = {
            let caps = surface.capabilities(device.physical_device())?;
            let alpha = caps
                .supported_composite_alpha
                .iter()
                .next()
                .ok_or("No supported alpha composite")?;
            let initial_dimensions =
                _dimensions(&surface.window()).ok_or("No window dimensions")?;

            Swapchain::new(
                device.clone(),
                surface.clone(),
                caps.min_image_count,
                caps.supported_formats[0].0,
                initial_dimensions,
                1,
                caps.supported_usage_flags,
                &window_queue.clone(),
                SurfaceTransform::Identity,
                alpha,
                PresentMode::Fifo,
                true,
                None,
            )?
        };

        Ok(SwapWindow {
            device,
            window_queue,
            surface: surface.clone(),
            dynamic_state: DynamicState { line_width: None, viewports: None, scissors: None },
            swapchain,
            swap_images,
        })
    }

    pub fn size_dependent_setup(
        &mut self,
        render_pass: Arc<RenderPassAbstract + Send + Sync>,
    ) -> Result<Vec<Arc<FramebufferAbstract + Send + Sync>>, FrameError> {
        let dimensions = self.swap_images[0].dimensions();
        let viewport = Viewport {
            origin: [0.0, 0.0],
            dimensions: [dimensions[0] as f32, dimensions[1] as f32],
            depth_range: 0.0..1.0,
        };
        self.dynamic_state.viewports = Some(vec![viewport]);
        // TODO duplicates code up above
        let frames = self
            .swap_images
            .iter()
            .map(|image| {
                Arc::new(
                    Framebuffer::start(render_pass.clone())
                        .add(image.clone())
                        .unwrap()
                        .build()
                        .unwrap(),
                ) as Arc<FramebufferAbstract + Send + Sync>
            })
            .collect::<Vec<Arc<FramebufferAbstract + Send + Sync>>>();
        Ok(frames)
    }

    pub fn recreate_swapchain(
        &mut self,
        render_pass: Arc<RenderPassAbstract + Send + Sync>,
    ) -> Result<Vec<Arc<FramebufferAbstract + Send + Sync>>, FrameError> {
        let dimensions = self.dimensions().ok_or("No window dimensions")?;
        let (new_swapchain, new_images) = match self.swapchain.recreate_with_dimension(dimensions) {
            Ok(r) => r,
            // This error tends to happen when the user is manually resizing the window.
            // Simply restarting the loop is the easiest way to fix this issue.
            Err(sce @ SwapchainCreationError::UnsupportedDimensions) => {
                return Err(FrameError::SwapchainCreation { sce });
            }
            Err(err) => panic!("{:?}", err),
        };

        self.swap_images = new_images;
        self.swapchain = new_swapchain;
        let frames = self.size_dependent_setup(render_pass.clone())?;
        return Ok(frames);
    }

    pub fn future_image(&self) -> Result<(usize, Box<GpuFuture>), FrameError> {
        match swapchain::acquire_next_image(self.swapchain.clone(), None) {
            Ok((image_index, future)) => Ok((image_index, Box::new(future))),
            Err(ae @ AcquireError::OutOfDate) | Err(ae @ AcquireError::SurfaceLost) => {
                return Err(FrameError::ImageAcquisition { ae });
            }
            Err(err) => panic!("{:?}", err),
        }
    }

    pub fn dimensions(&self) -> Option<[u32; 2]> {
        _dimensions(self.surface.window())
    }

    pub fn f_dimensions(&self) -> Option<[f32; 2]> {
        match self.surface.window().get_inner_size() {
            Some(dimensions) => {
                // convert to physical pixels
                let dimensions = dimensions.to_physical(self.surface.window().get_hidpi_factor());
                return Some([dimensions.width as f32, dimensions.height as f32]);
            }
            None => None,
        }
    }
}

#[inline]
fn _dimensions(window: &Window) -> Option<[u32; 2]> {
    match window.get_inner_size() {
        Some(dimensions) => {
            // convert to physical pixels
            let dimensions: (u32, u32) = dimensions.to_physical(window.get_hidpi_factor()).into();
            return Some([dimensions.0, dimensions.1]);
        }
        None => None,
    }
}

/// The Vulkan installation, the ICD's for devices, and the Vulkano Instance mainly provide
/// the entry point to getting and evaluating the capability of physical devices.
#[derive(Clone)]
pub struct GpuPicker {
    pub instance: Arc<vulkano::instance::Instance>,
}

impl GpuPicker {
    pub fn new() -> Result<GpuPicker, VulkanoError> {
        let app_info = vulkano::app_info_from_cargo_toml!();
        let extensions = vulkano_win::required_extensions();
        let instance = Instance::new(Some(&app_info), &extensions, None);
        return match instance {
            Ok(instance) => Ok(GpuPicker { instance }),
            Err(no_vulkan) => Err(VulkanoError::NoVulkanInstalled { ice: no_vulkan }),
        };
    }

    pub fn discrete_or_first_device(
        &self,
        surface: &Arc<Surface<Window>>,
    ) -> Result<PhysicalDevice, VulkanoError> {
        let all_devs = PhysicalDevice::enumerate(&self.instance);
        let mut can_draw =
            all_devs.filter(|&pd| GpuPicker::graphics_queue_fam(&pd, &surface).is_some());
        let mut discrete =
            can_draw.clone().filter(|&pd| -> bool { pd.ty() == PhysicalDeviceType::DiscreteGpu });
        if let Some(first_dev) = discrete.next() {
            Ok(first_dev)
        } else {
            warn!("No discrete device was found.");
            let first_dev = can_draw.next();
            match first_dev {
                Some(dev) => Ok(dev),
                None => {
                    return Err(VulkanoError::CantDraw {});
                }
            }
        }
    }

    pub fn compute_device(&self) -> Result<PhysicalDevice, Box<dyn Error>> {
        let all_devs = PhysicalDevice::enumerate(&self.instance);
        let mut can_compute = all_devs.filter(|&pd| GpuPicker::compute_queue_fam(&pd).is_some());
        let mut discrete = can_compute
            .clone()
            .filter(|&pd| -> bool { pd.ty() == PhysicalDeviceType::DiscreteGpu });
        if let Some(first_dev) = discrete.next() {
            Ok(first_dev)
        } else {
            warn!("No discrete device was found.");
            let first_dev = can_compute.next();
            match first_dev {
                Some(dev) => Ok(dev),
                None => {
                    return Err(Box::new(ENguyenError::from(
                        "No physical devices have compute capability",
                    )));
                }
            }
        }
    }

    pub fn has_graphics(device: &PhysicalDevice) -> bool {
        device.queue_families().find(|fam| fam.supports_graphics()).is_some()
    }

    pub fn has_compute(device: &PhysicalDevice) -> bool {
        device.queue_families().find(|fam| fam.supports_compute()).is_some()
    }

    pub fn graphics_queue_fam<'a>(
        pd: &'a PhysicalDevice,
        surface: &Arc<Surface<Window>>,
    ) -> Option<QueueFamily<'a>> {
        // Look for a supported queue family for the surface before assuming a device is acceptable
        pd.queue_families()
            .find(|fam| fam.supports_graphics() && surface.is_supported(*fam).unwrap_or(false))
    }

    pub fn compute_queue_fam<'a>(pd: &'a PhysicalDevice) -> Option<QueueFamily<'a>> {
        let mut has_compute = pd.queue_families().filter(|fam| fam.supports_compute());
        let mut compute_only =
            pd.queue_families().filter(|fam| !fam.supports_graphics() && fam.supports_compute());
        let first_dedicated = compute_only.next();
        if first_dedicated.is_some() {
            first_dedicated
        } else {
            has_compute.next()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulkano_win::VkSurfaceBuild;

    #[test]
    fn vulkan_installed() {
        GpuPicker::new().unwrap();
    }

    #[test]
    fn find_device_for_surface() {
        let picker = GpuPicker::new().unwrap();
        let surface = test_surface(&picker.instance);
        SwapWindow::new(&picker, &surface).unwrap();
    }

    #[test]
    fn get_dimensions() {
        let picker = GpuPicker::new().unwrap();
        let surface = test_surface(&picker.instance);
        let gpu_win = SwapWindow::new(&picker, &surface).unwrap();
        gpu_win.dimensions();
    }

    fn test_surface(instance: &Arc<Instance>) -> Arc<Surface<Window>> {
        let events_loop = winit::EventsLoop::new();
        winit::WindowBuilder::new()
            .with_title("E-Nguyen Test")
            .build_vk_surface(&events_loop, instance.clone())
            .unwrap()
    }

    #[test]
    fn compute_device_and_queue() {
        let picker = GpuPicker::new().unwrap();
        let pd = picker.compute_device().unwrap();
        let _queue_family = GpuPicker::compute_queue_fam(&pd).unwrap();
    }
}
