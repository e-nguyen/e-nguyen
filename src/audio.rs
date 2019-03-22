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

/// To support more platforms, find / create monitors or platform equivalents
/// and normalize their streams into the ring buffer that gets fed to the GPU.
///
/// If the audio server doesn't support an existing format that compute.rs can
/// handle, you will need to modify how the bytes are unpacked using the SimpleSource
/// information.
///
/// In general, monitor devices allow capturing audio without direct integration
/// to any particular media player.  On Linux, `pactl list` will show a monitor like so:
///
/// ```
/// Source #0
///      State: IDLE
///      Name: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor
///      Description: Monitor of Built-in Audio Analog Stereo
///      Driver: module-alsa-card.c
///      Sample Specification: s16le 2ch 44100Hz
///      Channel Map: front-left,front-right
///      Owner Module: 6
///      Mute: no
///      Volume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB
///              balance 0.00
///      Base Volume: 65536 / 100% / 0.00 dB
///      Monitor of Sink: alsa_output.pci-0000_00_1f.3.analog-stereo
///      Latency: 0 usec, configured 2000000 usec
///      Flags: DECIBEL_VOLUME LATENCY
///      Properties:
///          device.description = "Monitor of Built-in Audio Analog Stereo"
///          device.class = "monitor"
///          alsa.card = "0"
///          alsa.card_name = "HDA Intel PCH"
///          alsa.long_card_name = "HDA Intel PCH at 0xec428000 irq 133"
///          alsa.driver_name = "snd_hda_intel"
///          device.bus_path = "pci-0000:00:1f.3"
///          sysfs.path = "/devices/pci0000:00/0000:00:1f.3/sound/card0"
///          device.bus = "pci"
///          device.vendor.id = "8086"
///          device.vendor.name = "Intel Corporation"
///          device.product.id = "a171"
///          device.product.name = "CM238 HD Audio Controller"
///          device.form_factor = "internal"
///          device.string = "0"
///          module-udev-detect.discovered = "1"
///          device.icon_name = "audio-card-pci"
///      Formats:
///           pcm
///
/// Sample Specification: s16le 2ch 44100Hz
/// Signed 16-bit littel-endian 2 channel, 44100/s, so 176.4kbps raw PCM
/// ```
///
use crate::errors::ENguyenError;
use crate::ring::{RingBytes, RingReader};

use libpulse_binding as pulse;
use log::{debug, error, info, warn};
use pulse::callbacks::ListResult;
use pulse::context::introspect::SourceInfo;
use pulse::context::Context;
use pulse::def::BufferAttr;
use pulse::error::PAErr;
#[allow(unused_imports)]
use pulse::mainloop::api::Mainloop as MainloopTrait;
use pulse::mainloop::threaded::Mainloop;
use pulse::proplist::Proplist;
use pulse::sample::{Format, Spec};
use pulse::stream::flags;
use pulse::stream::{PeekResult, Stream};
use std::borrow::Cow;
use std::cell::RefCell;
use std::error::Error;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::thread::JoinHandle;
use std::time;

/// Implement AudioStream and adapt the input / output in compute to support additional
/// sound servers.
pub trait AudioStream {
    fn connect(&mut self) -> Result<RingState, Box<dyn Error>>;
    fn heat(&mut self) -> Result<(RingReader, SimpleSource), Box<dyn Error>>;
    fn chill(&mut self) -> Result<(RingState, JoinHandle<()>), Box<dyn Error>>;
    fn state(&self) -> RingState;
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum RingState {
    BORN,
    CONNECTED,
    HOT,
    DEAD,
}

/// minimal information necessary to correctly coerce a source to downstream readers
/// without relying on data backed by audio client/server memory, unsafe pointers etc
#[derive(Clone, Debug)]
pub struct SimpleSource {
    name: Box<String>,
    index: u32,
    rate: u32,
    channels: u8,
    sample_format: Format,
}

impl SimpleSource {
    fn from_pa_source_info(source_info: &SourceInfo) -> SimpleSource {
        let name = match &source_info.name {
            None => String::from("Unnamed audio source"),
            Some(Cow::Borrowed(inner_name)) => String::from(*inner_name),
            Some(Cow::Owned(inner_name)) => inner_name.clone(),
        };
        SimpleSource {
            name: Box::new(name),
            index: source_info.index,
            rate: source_info.sample_spec.rate,
            sample_format: source_info.sample_spec.format,
            channels: source_info.sample_spec.channels,
        }
    }

    /// Bytes per second.  Used to size buffers for a desired time window.
    pub fn byte_rate(&self) -> u64 {
        self.channels as u64 * self.rate as u64 * (self.sample_format.size()) as u64
    }

    /// Bytes per sample.  Used to size individual samples to calculate sample counts.
    fn sample_bytes(&self) -> u32 {
        self.channels as u32 * self.sample_format.size() as u32
    }
}

impl Default for SimpleSource {
    fn default() -> Self {
        SimpleSource {
            name: Box::new(String::from("Test simple source info")),
            index: 0,
            rate: 44100,
            sample_format: Format::S16le,
            channels: 2,
        }
    }
}

/// Test audio source
struct Square4kHz {
    volume: f32,
    hot_handle: Option<JoinHandle<()>>,
    frequency: u32,
    state: Mutex<RingState>,
    killed: Arc<AtomicBool>,
    source_info: SimpleSource,
    inverse_second_fraction: u32, // 100 -> 100ths of second between thread sleeps
}

impl Default for Square4kHz {
    fn default() -> Self {
        Square4kHz {
            volume: 0.5,
            hot_handle: None,
            frequency: 200,
            state: Mutex::new(RingState::BORN),
            killed: Arc::new(AtomicBool::from(false)),
            source_info: SimpleSource::default(),
            inverse_second_fraction: 100, // emit 100ths of a second
        }
    }
}

impl AudioStream for Square4kHz {
    fn connect(&mut self) -> Result<RingState, Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::BORN {
            Err(Box::new(ENguyenError::from("Ring already connected.  Get your own")))
        } else {
            // connect to server, don't start sending data yet
            *state = RingState::CONNECTED;
            Ok(RingState::CONNECTED)
        }
    }

    fn heat(&mut self) -> Result<(RingReader, SimpleSource), Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::CONNECTED {
            Err(Box::new(ENguyenError::from("Can't heat a ring that isn't connected")))
        } else {
            let (tx, rx) = RingBytes::new(16384);

            let killed: Weak<AtomicBool> = Arc::downgrade(&self.killed);
            let buffer_count = self.source_info.rate;
            let step_samples = (buffer_count / self.inverse_second_fraction) as usize;
            let inverse_second_fraction = self.inverse_second_fraction;
            let samples_per_cycle = (self.source_info.rate / (self.frequency * 2)) as usize;
            let r_diff = i16::max_value() as i32 - i16::min_value() as i32;
            let high_amplitude: i16 = (((r_diff / 2) as f32 * self.volume) as i32
                + i16::max_value() as i32
                - r_diff / 2) as i16;
            let low_amplitude: i16 = (((r_diff / 2) as f32 * -self.volume) as i32
                + i16::min_value() as i32
                + r_diff / 2) as i16;
            let mut is_high = true;
            let mut cycle_count: usize = 0;
            self.hot_handle = Some(thread::spawn(move || {
                loop {
                    if let Some(is_dead) = killed.upgrade() {
                        if is_dead.load(Ordering::Relaxed) {
                            break;
                        }
                    } else {
                        break;
                    }
                    let mut step_count: usize = 0;
                    while step_count < step_samples {
                        loop {
                            let cycle_remaining: usize = samples_per_cycle - cycle_count;
                            let step_remaining: usize = step_samples - step_count;
                            let remaining = {
                                if cycle_remaining > step_remaining {
                                    step_remaining
                                } else {
                                    cycle_remaining
                                }
                            };
                            let amplitude = if is_high { high_amplitude } else { low_amplitude };
                            let sample = unsafe {
                                // TODO use byteorder
                                // TODO check alignment
                                std::mem::transmute::<[i16; 2], [u8; 4]>([amplitude, amplitude])
                            };
                            let data: Vec<u8> = (0..remaining)
                                .into_iter()
                                .flat_map(|_i| sample.iter().cloned())
                                .collect();

                            // append bytes to available space and accumulate them
                            // until you have enough to send.
                            let space = tx.reserve(data.len());
                            tx.write(&data);

                            cycle_count += space / 4;
                            step_count += space / 4;
                            if cycle_count >= samples_per_cycle {
                                is_high = !is_high;
                                cycle_count = 0;
                                break;
                            } else if step_count >= step_samples {
                                break;
                            }
                        }
                    }
                    // between step writes, we sleep to simulate a
                    // realistic rate of audio samples
                    thread::sleep(time::Duration::from_millis(
                        1000 / inverse_second_fraction as u64,
                    ));
                }
            }));
            *state = RingState::HOT;
            Ok((rx, self.source_info.clone()))
        }
    }

    fn chill(&mut self) -> Result<(RingState, JoinHandle<()>), Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::HOT {
            Err(Box::new(ENguyenError::from("Can't chill a ring that isn't hot")))
        } else {
            self.killed.store(true, Ordering::Relaxed);
            *state = RingState::DEAD;
            info!("Audio ring successfully joined!");
            let mut handle = None;
            std::mem::swap(&mut self.hot_handle, &mut handle);
            Ok((RingState::DEAD, handle.unwrap()))
        }
    }

    fn state(&self) -> RingState {
        *self.state.lock().unwrap()
    }
}

/// Pulseaudio implementation
pub struct PaStream {
    hot_handle: Option<JoinHandle<()>>,
    state: Mutex<RingState>,
    killed: Arc<AtomicBool>,
    source_info: SimpleSource,
    source: ServerStream,
}

impl Default for PaStream {
    fn default() -> Self {
        let ac = connect_to_server().unwrap();
        let server_streams = server_streams(&ac);
        let (monitor, mon_info) = first_monitor(server_streams).unwrap();
        debug!("Using monitor: {:?}", monitor.name);
        ac.mainloop.borrow_mut().stop();
        PaStream {
            hot_handle: None,
            state: Mutex::new(RingState::BORN),
            killed: Arc::new(AtomicBool::from(false)),
            source_info: mon_info,
            source: monitor,
        }
    }
}

impl AudioStream for PaStream {
    fn connect(&mut self) -> Result<RingState, Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::BORN {
            Err(Box::new(ENguyenError::from("Ring already connected.  Get your own")))
        } else {
            // assert_eq!(connect_stream(&self.pa_context, &mut self.pa_stream, &self.source)?, true);
            *state = RingState::CONNECTED;
            Ok(RingState::CONNECTED)
        }
    }

    fn heat(&mut self) -> Result<(RingReader, SimpleSource), Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::CONNECTED {
            Err(Box::new(ENguyenError::from("Can't heat a ring that isn't connected")))
        } else {
            let weak_killed: Weak<AtomicBool> = Arc::downgrade(&self.killed);
            let min_count: usize = 128; // at least 128B at a time
            let (tx, rx) = RingBytes::new(32768);

            let monitor = self.source.clone();
            self.hot_handle = Some(thread::spawn(move || {
                let pa_context = connect_to_server().unwrap();
                let mut stream = create_stream(&pa_context, &monitor).unwrap();
                assert!(connect_stream(&pa_context, &mut stream, &monitor).unwrap());;
                let mut pa_stream = stream.lock().unwrap();
                pa_context.mainloop.borrow_mut().lock();
                pa_stream.uncork(None); // TODO wait on unlock
                pa_context.mainloop.borrow_mut().unlock();

                loop {
                    let killed_up = weak_killed.upgrade();
                    if killed_up.is_some() && !killed_up.unwrap().load(Ordering::Relaxed) {
                        pa_context.mainloop.borrow_mut().lock();
                        let avail = pa_stream.readable_size();
                        pa_context.mainloop.borrow_mut().unlock();
                        if let Some(count) = avail {
                            if count < min_count {
                                thread::sleep(time::Duration::from_micros(200));
                                continue;
                            }
                        }

                        let mut written = 0;
                        pa_context.mainloop.borrow_mut().lock();
                        let peek = pa_stream.peek().expect("Could not peek PA streag");
                        match peek {
                            PeekResult::Empty => {
                                pa_context.mainloop.borrow_mut().unlock();
                                thread::sleep(time::Duration::from_micros(200));
                                continue;
                            }
                            PeekResult::Hole(size) => {
                                debug!("Skipping PA stream hole sized: {:?}", size);
                                pa_stream.discard().unwrap();
                                pa_context.mainloop.borrow_mut().unlock();
                            }
                            PeekResult::Data(data) => {
                                let read = data.len();
                                let mut sentinel: i32 = 100;
                                while sentinel > 0 {
                                    let wavail = tx.reserve(data.len());
                                    if wavail > data.len() {
                                        tx.write(data);
                                        written = data.len();
                                    }
                                    sentinel -= 1;
                                    if written >= read {
                                        break;
                                    }
                                }
                                // done with the data
                                pa_stream.discard().expect("Could not discard PA stream");
                                pa_context.mainloop.borrow_mut().unlock();
                            }
                        }
                    } else {
                        let disconn = disconnect_stream(&pa_context, &stream);
                        match disconn {
                            Ok(_) => {}
                            Err(error) => warn!("Disconnect failed: {:?}", error),
                        }
                        pa_context.mainloop.borrow_mut().stop();
                        break;
                    }
                }
            }));
            *state = RingState::HOT;
            Ok((rx, self.source_info.clone()))
        }
    }

    fn chill(&mut self) -> Result<(RingState, JoinHandle<()>), Box<dyn Error>> {
        let mut state = self.state.lock().unwrap();
        if *state != RingState::HOT {
            Err(Box::new(ENguyenError::from("Can't chill a ring that isn't hot")))
        } else {
            self.killed.store(true, Ordering::Relaxed);
            *state = RingState::DEAD;
            let mut handle = None;
            std::mem::swap(&mut self.hot_handle, &mut handle);
            Ok((RingState::DEAD, handle.unwrap()))
        }
    }

    fn state(&self) -> RingState {
        *self.state.lock().unwrap()
    }
}

/// TODO ServerStream and SimpleSource can likely be merged
#[derive(Debug, Clone)]
pub struct ServerStream {
    name: String,
    index: u32,
    desc: String,
    spec: Spec,
}

enum ReadyState {
    Stream(pulse::stream::State),
    Context(pulse::context::State),
}

struct AudioContext {
    context: Rc<RefCell<Context>>,
    mainloop: Rc<RefCell<Mainloop>>,
}

fn connect_to_server() -> Result<AudioContext, String> {
    let app_name: &str = env!("CARGO_PKG_NAME");

    let mut proplist = Proplist::new().unwrap();
    proplist.sets(pulse::proplist::properties::APPLICATION_NAME, &app_name).unwrap();
    // TODO icons supported for apps like Pavucontrol
    // https://docs.rs/libpulse-binding/2.5.0/libpulse_binding/proplist/properties/constant.APPLICATION_ICON_NAME.html

    let mainloop = Rc::new(RefCell::new(Mainloop::new().expect("Failed to create mainloop")));
    let context = Rc::new(RefCell::new(
        Context::new_with_proplist(mainloop.borrow().deref(), &app_name, &proplist)
            .expect("Failed to create new context"),
    ));

    let ac = AudioContext { context, mainloop };

    {
        let ml_ref = Rc::clone(&ac.mainloop);
        let context_ref = Rc::clone(&ac.context);
        ac.context.borrow_mut().set_state_callback(Some(Box::new(move || {
            let state = unsafe { (*context_ref.as_ptr()).get_state() };
            match state {
                pulse::context::State::Ready
                | pulse::context::State::Failed
                | pulse::context::State::Terminated => unsafe {
                    (*ml_ref.as_ptr()).signal(false);
                },
                _ => {}
            }
        })));
    }

    ac.context
        .borrow_mut()
        .connect(None, pulse::context::flags::NOFLAGS, None)
        .expect("Failed to connect context");
    ac.mainloop.borrow_mut().lock();
    ac.mainloop.borrow_mut().start().expect("Failed to start mainloop");
    let state_closure = || ReadyState::Context(ac.context.borrow().get_state());
    ready_wait(&state_closure, &ac)?;
    ac.mainloop.borrow_mut().unlock();
    ac.context.borrow_mut().set_state_callback(None);
    Ok(ac)
}

fn server_streams(ac: &AudioContext) -> Vec<(ServerStream, SimpleSource)> {
    let found: Vec<(ServerStream, SimpleSource)> = Vec::with_capacity(10);
    let wrapped: Rc<RefCell<Vec<(ServerStream, SimpleSource)>>> = Rc::new(RefCell::new(found));
    let insider = wrapped.clone();
    ac.mainloop.borrow_mut().lock();
    let op = {
        let ml_ref = Rc::clone(&ac.mainloop);
        ac.context.borrow_mut().introspect().get_source_info_list(
            move |source_list: ListResult<&SourceInfo>| {
                match source_list {
                    ListResult::Item(source_info) => {
                        if let Some(name) = &source_info.name {
                            let desc: String = if let Some(d) = &source_info.description {
                                d.deref().to_owned()
                            } else {
                                "".to_string()
                            };
                            let s_source = SimpleSource::from_pa_source_info(source_info);
                            let s_stream = ServerStream {
                                name: name.to_string().clone(),
                                index: source_info.index,
                                spec: source_info.sample_spec.clone(),
                                desc,
                            };
                            insider.borrow_mut().push((s_stream, s_source));
                        } else {
                            debug!("Nameless device at index: {}", source_info.index);
                        }
                    }
                    ListResult::End => {
                        // This callback is executed once for each available device until
                        // returning ListResult::End
                        unsafe {
                            (*ml_ref.as_ptr()).signal(false);
                        }
                    }
                    ListResult::Error => {
                        error!("Listing devices failed, opaquely");
                        unsafe {
                            (*ml_ref.as_ptr()).signal(false);
                        }
                    }
                }
            },
        )
    };
    while op.get_state() == pulse::operation::State::Running {
        ac.mainloop.borrow_mut().wait();
    }
    ac.mainloop.borrow_mut().unlock();
    let unwrapped = wrapped.deref().borrow().clone();
    debug!("Input devices detected {:#?}", &unwrapped);
    unwrapped
}

fn first_monitor(
    devices: Vec<(ServerStream, SimpleSource)>,
) -> Option<(ServerStream, SimpleSource)> {
    for (dev, info) in devices.iter() {
        if dev.name.contains("monitor") || dev.name.contains("Monitor") {
            return Some((dev.clone(), info.clone()));
        }
    }
    return None;
}

fn create_stream(
    ac: &AudioContext,
    server_stream: &ServerStream,
) -> Result<Arc<Mutex<Stream>>, String> {
    // TODO check proplists again
    let stream = Arc::new(Mutex::new(
        Stream::new(&mut ac.context.borrow_mut(), "Music Monitor", &server_stream.spec, None)
            .expect("Failed to create new stream"),
    ));
    ac.mainloop.borrow_mut().lock();
    let ml_ref = Rc::clone(&ac.mainloop);
    // Stream state change callback
    {
        let weak_stream = Arc::downgrade(&stream);
        stream.lock().unwrap().set_state_callback(Some(Box::new(move || {
            match weak_stream.upgrade() {
                Some(stream) => {
                    // Setting the callback requires having the lock and can
                    // immediately execute on the same thread as setting the callback
                    if let Ok(res) = stream.try_lock() {
                        let state = res.get_state();
                        match state {
                            pulse::stream::State::Ready
                            | pulse::stream::State::Failed
                            | pulse::stream::State::Terminated => unsafe {
                                (*ml_ref.as_ptr()).signal(false);
                            },
                            _ => {}
                        }
                    } else {
                        warn!("PA state callback for Stream called with lock held");
                    }
                }
                None => {
                    warn!("Stream state callback for dropped stream");
                }
            }
        })));
    }
    ac.mainloop.borrow_mut().unlock();
    Ok(stream)
}

fn connect_stream(
    ac: &AudioContext,
    stream: &mut Arc<Mutex<Stream>>,
    stream_def: &ServerStream,
) -> Result<bool, String> {
    let ba = BufferAttr {
        maxlength: std::u32::MAX, // adjusts overall latency
        tlength: 2048,            // adjusts overall latency
        prebuf: std::u32::MAX,    // playback only
        minreq: std::u32::MAX,    // playback only
        fragsize: std::u32::MAX,  // adjusts overall latency
    };
    ac.mainloop.borrow_mut().lock();
    {
        stream
            .lock()
            .unwrap()
            .connect_record(
                Some(stream_def.name.as_str()),
                Some(&ba),
                flags::START_UNMUTED & flags::START_CORKED & flags::ADJUST_LATENCY,
            )
            .expect("Could not connect");
    }

    // Wait for stream to be ready
    let state_producer = || ReadyState::Stream(stream.clone().try_lock().unwrap().get_state());
    ready_wait(&state_producer, ac)?;
    ac.mainloop.borrow_mut().unlock();
    Ok(true)
}

fn disconnect_stream(ac: &AudioContext, stream: &Arc<Mutex<Stream>>) -> Result<bool, PAErr> {
    ac.mainloop.borrow_mut().lock();
    let mut s = stream.lock().unwrap();
    s.cork(None);
    s.flush(None);
    s.set_state_callback(None);
    s.disconnect()?;
    ac.mainloop.borrow_mut().unlock();
    Ok(true)
}

fn ready_wait(state_closure: &Fn() -> ReadyState, ac: &AudioContext) -> Result<(), String> {
    loop {
        let ml = &ac.mainloop;
        match state_closure() {
            ReadyState::Stream(state) => match state {
                pulse::stream::State::Ready => {
                    break;
                }
                pulse::stream::State::Failed | pulse::stream::State::Terminated => {
                    ml.borrow_mut().unlock();
                    ml.borrow_mut().stop();
                    return Err(
                        "PulseAudio returned Failed|Terminated.  Check sound server.".to_owned()
                    );
                }
                _ => {
                    ml.borrow_mut().wait();
                }
            },
            ReadyState::Context(state) => match state {
                pulse::context::State::Ready => {
                    break;
                }
                pulse::context::State::Failed | pulse::context::State::Terminated => {
                    ml.borrow_mut().unlock();
                    ml.borrow_mut().stop();
                    return Err(
                        "PulseAudio returned Failed|Terminated.  Check sound server.".to_owned()
                    );
                }
                _ => {
                    ml.borrow_mut().wait();
                }
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pa_raw_tests() {
        let ac = connect_to_server().unwrap();
        let streams = server_streams(&ac);
        let (monitor, _monitor_info) = first_monitor(streams).unwrap();
        let mut stream = create_stream(&ac, &monitor).unwrap();
        connect_stream(&ac, &mut stream, &monitor).unwrap();
        disconnect_stream(&ac, &stream).unwrap();
    }

    #[test]
    fn heat_and_chill_square_test_ring() {
        let min_count = 512;
        let mut stream = Square4kHz::default();
        let _connected = stream.connect().unwrap();
        let (rx, source) = stream.heat().unwrap();
        assert_eq!(source.byte_rate(), 44100 * 4);
        let mut recorded = 0;

        let handle = thread::spawn(move || {
            while recorded < 16334 {
                let ravail = rx.available();
                if ravail > min_count {
                    let _read = rx.read(ravail);
                    recorded += ravail;
                }
            }
        });
        handle.join().unwrap();
        stream.chill().unwrap().1.join().unwrap();
    }

    #[test]
    fn heat_and_chill_pa_ring() {
        let min_count = 512;
        let mut stream = PaStream::default();

        let _connected = stream.connect().unwrap();
        let (rx, source) = stream.heat().unwrap();
        assert_eq!(source.byte_rate(), 44100 * 4);
        let mut recorded = 0;

        let handle = thread::spawn(move || {
            while recorded < 16334 {
                let ravail = rx.available();
                if ravail > min_count {
                    let _read = rx.read(ravail);
                    recorded += ravail;
                }
            }
        });
        handle.join().unwrap();
        stream.chill().unwrap().1.join().unwrap();
    }
}
