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

use crate::ewin::SwapWindow;

use std::error::Error;

pub mod placeholder_vsm {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "
#version 450

layout(location = 0) in vec2 position;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
}",

    }
}

pub mod placeholder_fsm {
    vulkano_shaders::shader! {
    ty: "fragment",
        src: "
#version 450

layout(location = 0) out vec4 f_color;

void main() {
    f_color = vec4(0.1, 0.2, 0.3, 1.0);
}
"
    }
}

pub mod uv_image_vsm {
    vulkano_shaders::shader! {
    ty: "vertex",
        src: "
#version 450

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 uv;
layout(location = 0) out vec2 tex_coords;

void main() {
    gl_Position = vec4(position, 0.0, 1.0);
    tex_coords = uv;
}"
    }
}

pub mod uv_image_fsm {
    vulkano_shaders::shader! {
    ty: "fragment",
        src: "
#version 450

layout(location = 0) in vec2 tex_coords;
layout(location = 0) out vec4 f_color;

layout(set = 0, binding = 0) uniform sampler2D tex;

void main() {
    f_color = texture(tex, tex_coords);
}"
    }
}

pub mod diag_grad_vsm {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "
#version 450

layout(location = 0) in vec2 position;
layout(location = 0) out vec2 pos;

void main() {
    pos = position; // to avoid plumbing screen size and using gl_FragCoord
    gl_Position = vec4(position, 0.0, 1.0);
}",

    }
}

pub mod diag_grad_fsm {
    vulkano_shaders::shader! {
    ty: "fragment",
        src: "
#version 450

// Make-shift full-screen diagonal gradient.  Use the distance from top-left to bottomr-right
// and normalize it to the full distance to calculate a linear gradient mixture factor. Designed
// for use on full-screen rectangle

// layout(origin_upper_left) in vec4 gl_FragCoord;
layout(location = 0) in vec2 pos;
layout(location = 0) out vec4 f_color;

const vec4 blue = vec4(0.002, 0.241, 0.5, 1.0);
const vec4 green = vec4(0, 0.906, 0.702, 1.0);

void main() {
    // distance from top left == 2.83 -> full mixture
    // TODO use a dot product
    float distance = pow(pow(pos.x - (-1.0), 2.0)  + pow(pos.y - (-1.0), 2.0), 0.5);
    float mix_fac = (distance / 2.83);
    f_color = mix(blue, green, mix_fac);
}"
    }
}

#[derive(Debug, Clone)]
pub struct XyUvVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}
vulkano::impl_vertex!(XyUvVertex, position, uv);

#[derive(Debug, Clone)]
pub struct XyUvRgbaVertex {
    // used for drawing solid colors
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub rgba: [f32; 4],
}
vulkano::impl_vertex!(XyUvRgbaVertex, position, uv, rgba);

#[derive(Debug, Clone)]
pub struct XyVertex {
    pub position: [f32; 2],
}
vulkano::impl_vertex!(XyVertex, position);

pub trait Frame {
    fn size_dependent_setup(&mut self) -> Result<(), Box<dyn Error>>;
    fn recreate_swapchain(&mut self, context: &SwapWindow) -> Result<(), Box<dyn Error>>;
    fn render_one(&mut self, context: &SwapWindow, recreate: &bool) -> Result<(), Box<dyn Error>>;
}

/// Less dynamic data such as the products of pipeline setup and command buffer pool
/// creation.  More tightly associated with the GPU.
pub trait Framer<'f, 'r: 'f, F, S, R> {
    fn render_one(
        &mut self,
        swap_win: &mut SwapWindow,
        state: S,
        resources: &R,
    ) -> Result<S, Box<dyn Error>>;
    fn new(swap_win: &mut SwapWindow, resources: &'r R) -> Result<(F, S), Box<dyn Error>>
    where
        F: Framer<'f, 'r, F, S, R>,
        S: FrameState,
        R: Send; // generous bound
}

/// Mutable datastructure that allows rich interaction with application flow control
/// between frames.  More tightly associated with the window and swapchain.
pub trait FrameState {}
