//! This is little more than a wrapper around the replaygain analysis functionality
//! provided by ffmpeg's af_replaygain. Unlike af_replaygain however, this gives you
//! the actual values instead of just printing them to the console and then throwing
//! them away. So close, yet so far.
//!
//! # Prerequisites
//!
//! * Stereo audio (no other channel counts supported)
//! * Supported sample rates: 8000, 11025, 12000, 16000, 18900, 22050, 24000, 32000,
//!   37800, __44100__, __48000__, 56000, 64000, 88200, 96000, 112000, 128000, 144000,
//!   176400, 192000 (Hz)
//! * Float encoding (endianness handled on your side)
//!
//! It sure doesn't lack irony that most users of this crate would probably actually
//! use ffmpeg to convert their audio to a compatible format.
//!
//! # Usage
//!
//! ```no_run
//! use replaygain::ReplayGain;
//! let mut rg = ReplayGain::new(44100).unwrap();
//! let samples = []; // get data from somewhere
//! rg.process_frame(&samples);
//! let (gain, peak) = rg.finish();
//! ```
//!
//! # Example
//!
//! ```no_run
//! use std::{env, io, slice};
//! use std::io::Read;
//! use replaygain::ReplayGain;
//!
//! fn main() {
//!     let mut args = env::args();
//!     args.next().unwrap(); // executable name
//!     let param1 = args.next().unwrap();
//!
//!     let sample_rate = param1.parse().unwrap();
//!     let mut rg = ReplayGain::new(sample_rate).unwrap();
//!
//!     // Just buffer everything up to keep things simple
//!     let mut input = Vec::new();
//!     {
//!         let stdin = io::stdin();
//!         let mut lock = stdin.lock();
//!         lock.read_to_end(&mut input).unwrap();
//!     }
//!
//!     // Quick and dirty conversion
//!     let floats = unsafe { slice::from_raw_parts(&input[..] as *const _ as *const f32,
//!                                                 input.len() / 4) };
//!     rg.process_samples(floats);
//!
//!     let (gain, peak) = rg.finish();
//!     println!("track_gain = {} dB", gain);
//!     println!("track_peak = {}", peak);
//! }
//! ```



mod af_replaygain;
use af_replaygain::*;

pub struct ReplayGain {
    sample_rate: usize,
    ctx: ReplayGainContext,
    buf: Vec<f32>,
}

impl ReplayGain {
    /// Create a new ReplayGain filter for the given sample rate.
    /// Returns `None` if the sample rate is not supported.
    pub fn new(sample_rate: usize) -> Option<ReplayGain>{
        freq_to_info(sample_rate).map(|x| ReplayGain {
            sample_rate,
            ctx: init_context(&x),
            buf: Vec::new(),
        })
    }

    /// Returns the size of a single audio frame (one of which we analyze at a time)
    /// in **floats**. Note that because we expect stereo audio, this means that you
    /// need to divide this by 2 to get the number of *samples*.
    pub fn frame_size(&self) -> usize {
        self.sample_rate / 20 * 2
    }

    /// Processes a single audio frame.
    ///
    /// # Panics
    ///
    /// Panics if `frame.len() != self.frame_size()` or if there's anything in
    /// `process_samples`'s buffer.
    /// If you need buffering, use `process_samples()` and **only that** instead.
    pub fn process_frame(&mut self, frame: &[f32]) {
        assert!(frame.len() == self.frame_size());
        assert!(self.buf.is_empty());

        filter_frame(&mut self.ctx, frame);
    }

    /// Processes a given amount of audio samples.
    ///
    /// Note that because we expect stereo audio, it doesn't actually make sense to pass
    /// an odd number of floats to this function but we buffer it to chunks of `frame_size()`
    /// anyways so we don't care.
    pub fn process_samples(&mut self, frame: &[f32]) {
        let frame_size = self.frame_size();
        let mut remainder = None;

        if !self.buf.is_empty() {
            // need to drain that first
            let required = frame_size - self.buf.len();
            let can_fill = frame.len() >= required;
            let input = if can_fill { required } else { frame.len() };
            self.buf.extend_from_slice(&frame[..input]);
            if can_fill {
                assert!(self.buf.len() == frame_size);
                filter_frame(&mut self.ctx, &self.buf[..]);
                self.buf.clear();
                remainder = Some(&frame[input..]);
            }
        } else {
            remainder = Some(frame);
        }

        for chunk in remainder.iter().flat_map(|x| x.chunks(frame_size)) {
            if chunk.len() == frame_size {
                assert!(self.buf.is_empty());
                filter_frame(&mut self.ctx, chunk);
            } else {
                // last one
                self.buf.extend_from_slice(chunk);
            }
        }
    }

    /// Completes the analysis and returns the two replaygain values (gain, peak).
    pub fn finish(mut self) -> (f32, f32) {
        // pass in any remaining buffer after padding with zeros
        self.buf.resize(self.frame_size(), 0.0);
        filter_frame(&mut self.ctx, &self.buf[..]);
        self.buf.clear();

        finish(&mut self.ctx)
    }
}
