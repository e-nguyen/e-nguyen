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

use failure::Fail;
use log::error;
use std::convert::From;
use std::error::Error as OldError;
use std::fmt;
use vulkano::memory::DeviceMemoryAllocError;
use vulkano::swapchain::{AcquireError, CapabilitiesError, SwapchainCreationError};

/// Checked errors originating from Vulkan setup or Vulkano API
#[derive(Debug, Fail)]
pub enum VulkanoError {
    #[fail(display = "No Vulkan implementation installed: {}", ice)]
    NoVulkanInstalled { ice: vulkano::instance::InstanceCreationError },
    #[fail(display = "Device creation error: {}", dce)]
    DeviceCreation { dce: vulkano::device::DeviceCreationError },
    #[fail(display = "Device capabilities error: {}", dce)]
    DeviceCapabilities { dce: CapabilitiesError },
    #[fail(display = "No device could draw to the window")]
    CantDraw {},
    #[fail(display = "Device out of memory: {}", doom)]
    DeviceOom { doom: DeviceMemoryAllocError },
    #[fail(display = "No display detected.  What do?")]
    NoDisplay {},
    #[fail(display = "Fatal: {}", msg)]
    Fatal { msg: &'static str },
    #[fail(display = "SwapchainCreationError {}", sce)]
    SwapchainCreation { sce: SwapchainCreationError },
}

impl From<vulkano::device::DeviceCreationError> for VulkanoError {
    fn from(dce: vulkano::device::DeviceCreationError) -> VulkanoError {
        VulkanoError::DeviceCreation { dce }
    }
}

impl From<FrameError> for VulkanoError {
    fn from(err: FrameError) -> VulkanoError {
        VulkanoError::Fatal { msg: "Frame error in unrecoverable position" }
    }
}

impl From<&'static str> for VulkanoError {
    fn from(msg: &'static str) -> VulkanoError {
        VulkanoError::Fatal { msg }
    }
}

impl From<CapabilitiesError> for VulkanoError {
    fn from(dce: CapabilitiesError) -> VulkanoError {
        VulkanoError::DeviceCapabilities { dce }
    }
}

impl From<SwapchainCreationError> for VulkanoError {
    fn from(sce: SwapchainCreationError) -> VulkanoError {
        VulkanoError::SwapchainCreation { sce }
    }
}

impl From<DeviceMemoryAllocError> for VulkanoError {
    fn from(doom: DeviceMemoryAllocError) -> VulkanoError {
        VulkanoError::DeviceOom { doom }
    }
}

impl From<vulkano::command_buffer::CommandBufferExecError> for VulkanoError {
    fn from(_ugh: vulkano::command_buffer::CommandBufferExecError) -> VulkanoError {
        VulkanoError::Fatal { msg: "Fixme" }
    }
}

impl From<Box<OldError>> for VulkanoError {
    fn from(err: Box<OldError>) -> VulkanoError {
        error!("(This is a placeholder) Error: {}", err);
        VulkanoError::Fatal { msg: "Placeholder error.  Check console error logs" }
    }
}

/// Recoverable frame errors
#[derive(Debug, Fail)]
pub enum FrameError {
    #[fail(display = "Swapchain buffers require updating before next image acquisition: {}", ae)]
    ImageAcquisition { ae: AcquireError },
    #[fail(display = "Swapchain creation failed.  Recreate: {}", sce)]
    SwapchainCreation { sce: SwapchainCreationError },
    #[fail(display = "Generic error: {}", msg)]
    Generic { msg: &'static str },
}

impl From<&'static str> for FrameError {
    fn from(msg: &'static str) -> FrameError {
        FrameError::Generic { msg }
    }
}

/// Custom error for switching known error behavior.
#[derive(Debug, Clone)]
pub struct ENguyenError {
    message: String,
}

impl fmt::Display for ENguyenError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ENguyenError: {}", self.message)
    }
}

impl From<&'static str> for ENguyenError {
    fn from(s: &'static str) -> ENguyenError {
        ENguyenError { message: s.to_owned() }
    }
}

impl OldError for ENguyenError {
    fn description(&self) -> &str {
        // https://doc.rust-lang.org/std/error/trait.Error.html
        // This method is soft-deprecated.
        "ENguyenError!"
    }
}
