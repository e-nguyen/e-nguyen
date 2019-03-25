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

use crate::audio::{AudioStream, PaStream};

use byteorder::ByteOrder;
use byteorder::LittleEndian as Le;
use bytes::buf::BufMut;
use bytes::BytesMut;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use rustfft::FFTplanner;
use std::boxed::Box;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time;
use vulkano::buffer::{BufferUsage, CpuBufferPool};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::device::{Device, Queue};
use vulkano::format::Format;
use vulkano::image::{Dimensions, ImageUsage, StorageImage};
use vulkano::pipeline::ComputePipeline;
use vulkano::sync;
use vulkano::sync::GpuFuture;

/// The compute module provides processed audio in the form of a channel of textures
/// and their futures.  Implement as an AudioTexTap that provides a stream of AudioTex.

/// Consume these by memory barrier synchronization and copy (or use directly)
pub struct AudioTex {
    pub buffer: Arc<StorageImage<Format>>,
    pub ready: Box<dyn GpuFuture + Send + Sync>,
}

pub struct AudioTexSource {
    tex_height: usize,
    bins: usize,
}

impl AudioTexSource {
    pub fn new(height: usize) -> Result<AudioTexSource, Box<dyn Error>> {
        let padded_bins = height * 2;
        let tex_height = height;
        Ok(AudioTexSource { tex_height: height, bins: padded_bins })
    }
}

/// This trait describes a source of audio textures that renderers
/// can tap into for use in drawing things that are Nguyen
pub struct AudioTexTap {
    hot_handle: Option<JoinHandle<()>>,
    killed: Arc<AtomicBool>,
    pub tap: mpsc::Receiver<AudioTex>,
}

impl AudioTexTap {
    pub fn turn_on(
        source: AudioTexSource,
        device: Arc<Device>,
        compute_queue: Arc<Queue>,
    ) -> Result<AudioTexTap, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let killed = Arc::new(AtomicBool::new(false));
        let kill_watch = killed.clone();

        let hot_handle = thread::spawn(move || {
            let draw_log_scale = LogScale::new(source.tex_height, 40_f64, MAX_AUDIBLE);

            let mut left_input: Vec<Complex<f32>> = vec![Zero::zero(); source.bins];
            let mut right_input: Vec<Complex<f32>> = vec![Zero::zero(); source.bins];
            let mut output: Vec<Complex<f32>> = vec![Zero::zero(); source.bins];

            let mut planner = FFTplanner::new(false);
            let fft = planner.plan_fft(source.bins);
            let fft_bufpool: CpuBufferPool<Complex<f32>> =
                CpuBufferPool::new(device.clone(), BufferUsage::all());
            let mut pastream = PaStream::default();
            pastream.connect().unwrap();
            let (rx, source_def) = pastream.heat().unwrap();
            let byte_rate = source_def.byte_rate();
            let target_bytes_per_frame = (byte_rate / 60) as usize;
            let fft_byte_len: usize = source.bins * 4; // Complex<f32>
            let mut stream_buf =
                BytesMut::with_capacity(target_bytes_per_frame * 6 + 32 * source.bins);
            let mut audio: Vec<i16> = vec![0; source.bins * 2];

            let lin_fft_res = (source_def.rate / 2) as f64 / (source.bins / 2) as f64; // Nyquist limit / nbins

            let norm = 1.0 / (i16::max_value() as f32);

            // compute an output texture and yield the AudioTex
            let shader = channel_combine::Shader::load(device.clone()).unwrap();
            let pipeline = Arc::new(
                ComputePipeline::new(device.clone(), &shader.main_entry_point(), &()).unwrap(),
            );

            while !kill_watch.load(Ordering::Relaxed) {
                let avail = rx.available();

                if avail < (target_bytes_per_frame * 2) {
                    thread::sleep(time::Duration::from_micros(500));
                    continue;
                }

                let mut to_consume = if avail > target_bytes_per_frame * 2 {
                    target_bytes_per_frame
                } else {
                    continue;
                };
                to_consume -= to_consume % 4;

                let fresh_bytes = rx.read(to_consume);
                stream_buf.reserve(to_consume);
                stream_buf.put(&fresh_bytes);
                let fft_available = stream_buf.len();
                if fft_available > fft_byte_len {
                    stream_buf.advance(fft_available - fft_byte_len);
                }

                if stream_buf.len() < fft_byte_len {
                    continue;
                }

                {
                    Le::read_i16_into(&stream_buf.clone().split_to(fft_byte_len), &mut audio);
                    let mut lc = left_input.iter_mut();
                    let mut rc = right_input.iter_mut();
                    for sample in audio.chunks_exact(2) {
                        let normed = sample[1] as f32 * norm;
                        *lc.next().unwrap() = Complex::new(normed, 0.0);
                        let normed = sample[0] as f32 * norm;
                        *rc.next().unwrap() = Complex::new(normed, 0.0);
                    }
                }

                fft.process(&mut left_input, &mut output);
                let left_buffer = fft_bufpool.chunk(output.clone().into_iter()).unwrap();
                fft.process(&mut right_input, &mut output);
                let right_buffer = fft_bufpool.chunk(output.clone().into_iter()).unwrap();

                let out_buf = StorageImage::with_usage(
                    device.clone(),
                    Dimensions::Dim2d { width: 1, height: source.tex_height as u32 },
                    Format::R32G32B32A32Sfloat,
                    ImageUsage { transfer_source: true, storage: true, ..ImageUsage::none() },
                    vec![compute_queue.family()],
                )
                .unwrap();

                let set = Arc::new(
                    PersistentDescriptorSet::start(pipeline.clone(), 0)
                        .add_buffer(left_buffer.clone())
                        .unwrap()
                        .add_buffer(right_buffer.clone())
                        .unwrap()
                        .add_image(out_buf.clone())
                        .unwrap()
                        .build()
                        .unwrap(),
                );

                let push_constants = channel_combine::ty::PushConstant {
                    lin_bins: source.bins as u32,
                    log_scale: draw_log_scale.log_bin_ratio as f32,
                    lin_res: lin_fft_res as f32,
                    min_freq: draw_log_scale.min_freq as f32,
                    max_freq: draw_log_scale.max_freq as f32,
                };

                let cb = AutoCommandBufferBuilder::secondary_compute_simultaneous_use(
                    device.clone(),
                    compute_queue.family(),
                )
                .unwrap();

                assert_eq!(source.tex_height as u32 % channel_combine::LOCAL_SIZE_X, 0);
                let dispatch_x = source.tex_height as u32 / channel_combine::LOCAL_SIZE_X;
                let cb = cb
                    .dispatch([dispatch_x, 1, 1], pipeline.clone(), set.clone(), push_constants)
                    .unwrap()
                    .build()
                    .unwrap();
                let future =
                    sync::now(device.clone()).then_execute(compute_queue.clone(), cb).unwrap();
                let result = AudioTex { ready: Box::new(future), buffer: out_buf.clone() };
                tx.send(result).unwrap();
            }
        });

        Ok(AudioTexTap { killed, hot_handle: Some(hot_handle), tap: rx })
    }
}

impl Drop for AudioTexTap {
    fn drop(&mut self) {
        self.killed.store(true, Ordering::Relaxed);
        let mut handle: Option<JoinHandle<_>> = None;
        std::mem::swap(&mut handle, &mut self.hot_handle);
        if let Some(hot) = handle {
            hot.join().unwrap();
        }
    }
}

static MAX_AUDIBLE: f64 = 20000_f64;
static MIN_AUDIBLE: f64 = 20_f64;

struct LogScale {
    n_log_bins: usize,
    n_lin_bins: usize,
    log_bin_ratio: f64,
    min_freq: f64,
    max_freq: f64,
}

impl LogScale {
    fn new(log_bins: usize, f_min: f64, f_max: f64) -> LogScale {
        assert!(f_min < MAX_AUDIBLE);
        assert!(f_max > MIN_AUDIBLE);
        assert!(f_max > f_min);

        let f_max = if f_max > MAX_AUDIBLE { MAX_AUDIBLE } else { f_max };
        let f_min = if f_min < MIN_AUDIBLE { MIN_AUDIBLE } else { f_min };

        let f64_log_bins = log_bins as f64;

        // the (1 / n_log_bins) power takes the n_log_binsth root which will
        // make a the ratio of successors of our logorithmic frequency bins
        let a = (f_max / f_min).powf(1.0_f64 / (f64_log_bins - 1_f64));

        // worst-case resolution
        let min_log_res = f_min * (a - 1.0_f64);
        let n_lin_bins = f_max / min_log_res;
        LogScale {
            n_log_bins: log_bins,
            n_lin_bins: n_lin_bins as usize,
            log_bin_ratio: a,
            min_freq: f_min,
            max_freq: f_max,
        }
    }
}

mod channel_combine {
    pub static LOCAL_SIZE_X: u32 = 16; // this must match local size
    vulkano_shaders::shader! {
        ty: "compute",
        src: "
#version 450

const float HAPI = 1.5707963267948966;
const float IPI = 0.3183098861837907;

struct Complex {
    float real;
    float imag;
};

layout(local_size_x=16, local_size_y=1, local_size_z=1) in;
layout(set = 0, binding = 0) buffer LeftData {Complex data[];} left_chan;
layout(set = 0, binding = 1) buffer RightData {Complex data[];} right_chan;
layout (set = 0, binding = 2, rgba32f)  uniform image2D out_img;
layout (push_constant) uniform PushConstant {
    uint lin_bins;
    float log_scale;
    float lin_res;
    float min_freq;
    float max_freq;
} fft;

float norm_tan(float unnormed);
float mag(Complex c);
float phase(Complex c);

void main() {
    uint gidx = gl_GlobalInvocationID.x;
    uint widx = gl_WorkGroupID.x;
    uint lidx = gl_LocalInvocationID.x;
    uint num_groups = gl_NumWorkGroups.x;
    uint woven = widx + lidx * num_groups;

    float left_sum = 0.0;
    float right_sum = 0.0;

    {
        float freq_center = fft.min_freq * pow(fft.log_scale, float(woven));

        float freq_start = fft.min_freq * pow(fft.log_scale, float(woven) + 0.5);
        uint start_tidx = uint(freq_start / fft.lin_res);
        float freq_end = fft.min_freq * pow(fft.log_scale, float(woven) + 1.5);
        uint end_tidx = uint(freq_end / fft.lin_res);

        // fractional samples
        float start_bin_start_freq = float(start_tidx - 1) * fft.lin_res;
        float start_bin_end_freq = float(start_tidx + 1) * fft.lin_res;
        float start_frac = (start_bin_end_freq - freq_start) / 
                           (start_bin_end_freq - start_bin_start_freq);
        float end_bin_start_freq = float(end_tidx - 1) * fft.lin_res;
        float end_bin_end_freq = float(end_tidx + 1) * fft.lin_res;
        float end_frac = (freq_end - end_bin_start_freq) /
                         (end_bin_end_freq - end_bin_start_freq);

        float tex_frac = start_frac;
        uint tidx = start_tidx;
        while (tidx >= start_tidx && tidx <= end_tidx) {
            if (start_tidx == end_tidx) {
                tex_frac = 1.0;
            }
            uint conjugate_index = fft.lin_bins - 1 - start_tidx;
            uint complex_index = start_tidx;
            Complex conj_l = left_chan.data[conjugate_index];
            Complex conj_r = right_chan.data[conjugate_index];

            Complex com_l = left_chan.data[complex_index];
            Complex com_r = right_chan.data[complex_index];
        
            float mag_l = (mag(com_l) + mag(conj_l)) * 0.5;
            float mag_r = (mag(com_r) + mag(conj_r)) * 0.5;

            left_sum += mag_l * tex_frac;
            right_sum += mag_r * tex_frac;
            tidx++;
            if (tidx == end_tidx) {
                tex_frac = end_frac;
            } else {
                tex_frac = 1.0;
            }
        }
    }

    vec4 out_col = vec4(0.4 * (right_sum - 1.5), 
                        0.1 * (pow(left_sum * right_sum, 0.5) - 4.0),
                        0.8 *(left_sum - 3.0),
                        1.0);

    imageStore(out_img, ivec2(0, woven), out_col);
}

// TODO this mapping is suspicious
float norm_tan(float unnormed) {
    return (unnormed + HAPI) / IPI;
}

// magnitude
float mag(Complex c) {
    return pow((pow(c.real, 2.0) + pow(c.imag, 2.0)), 0.5);
}

// phase
float phase(Complex c) {
    return c.real != 0.0 ? norm_tan(atan(c.imag / c.real)) : 0.0;
}
"

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO re-implement tests with updated signature
}
