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

use bytes::buf::BufMut;
use bytes::{Bytes, BytesMut};
use std::sync::{Arc, Mutex};

pub type Guarantee = usize; // guardrail requesting more than available

#[derive(Clone)]
pub struct RingBytes {
    buf: Arc<Mutex<BytesMut>>,
}

/// 2.4GB/s is enough for 44.1KB/s but monotonic lock-free would be better
impl RingBytes {
    pub fn new(size: usize) -> (RingWriter, RingReader) {
        let buf = BytesMut::with_capacity(size);
        let ring = RingBytes { buf: Arc::new(Mutex::new(buf)) };
        (RingWriter { ring: ring.clone() }, RingReader { ring: ring.clone() })
    }
}

pub struct RingReader {
    ring: RingBytes,
}

impl RingReader {
    pub fn available(&self) -> Guarantee {
        self.ring.buf.lock().unwrap().len()
    }

    pub fn read(&self, amount: usize) -> Bytes {
        self.ring.buf.lock().unwrap().split_to(amount).freeze()
    }
}

pub struct RingWriter {
    ring: RingBytes,
}

impl RingWriter {
    pub fn reserve(&self, size: usize) -> Guarantee {
        let mut buf = self.ring.buf.lock().unwrap();
        buf.reserve(size);
        buf.remaining_mut()
    }

    pub fn write(&self, bytes: &[u8]) {
        let mut buf = self.ring.buf.lock().unwrap();
        buf.put(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Instant;

    #[test]
    pub fn send_a_megabyte() {
        let one_meg: usize = 1048576; // 1mb
        let (tx, rx) = RingBytes::new(7777); // ???kb
        let (ctx, crx) = mpsc::sync_channel(2);
        let test_data: [u8; 1024] = [0; 1024]; // 1kb

        let handle = thread::spawn(move || {
            let mut read: usize = 0;
            loop {
                let avail = rx.available();
                let received_len = rx.read(avail).len();
                read += received_len;
                ctx.send(received_len).unwrap();
                if read >= one_meg {
                    break;
                }
            }
        });

        let mut written: usize = 0;
        let mut received: usize = 0;
        let before_test = Instant::now();
        loop {
            let avail = tx.reserve(test_data.len());
            if avail >= test_data.len() {
                if written < one_meg {
                    tx.write(&test_data);
                    written += test_data.len();
                }
            }
            match crx.try_recv() {
                Ok(received_len) => {
                    received += received_len;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    break;
                }
                _ => {}
            }
        }
        let duration = Instant::now().duration_since(before_test);
        println!("Time for 1MB: {:?}μs", duration.as_micros());
        println!("Buffer capacity afterwards: {:?}", tx.ring.buf.lock().unwrap().capacity());
        assert!(written >= one_meg);
        assert!(received >= one_meg);
        assert!(written == received);
        handle.join().unwrap();
    }
}
