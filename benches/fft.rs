#[macro_use]
extern crate criterion;

use criterion::Criterion;

use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
use rustfft::{FFTnum, FFTplanner, FFT};
use std::time::{Duration, Instant};

fn fft_process_one_frame() {
    // Release performance was found ~100x the desired throughput for 60fps
    // FFT's complexity class are all O(n log(n)) so using larger windows
    // won't break your CPU budget too quickly.

    // this is a two-channel example for a tex-height of 1024
    let bin_count = 2088; // dropping 20 lower bins of halved result
    let target_fps = 60;

    let mut left_input: Vec<Complex<f32>> = vec![Zero::zero(); bin_count];
    let mut right_input: Vec<Complex<f32>> = vec![Zero::zero(); bin_count];
    let mut output: Vec<Complex<f32>> = vec![Zero::zero(); bin_count];

    let mut planner = FFTplanner::new(false);
    let fft = planner.plan_fft(bin_count);

    let before_test = Instant::now();
    fft.process(&mut left_input, &mut output);
    // println!("First complex in output: {:?}", output.get(0).unwrap());
    fft.process(&mut right_input, &mut output);
    // println!("First complex in output: {:?}", output.get(0).unwrap());
    let duration = Instant::now().duration_since(before_test);
    // println!("Time for {:?} frames: {:?}ms", target_fps, duration.as_millis() * target_fps);
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("fft_process_one_frame", |b| b.iter(|| fft_process_one_frame()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
