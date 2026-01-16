//! Audio output service for playback and monitoring
//!
//! Provides audio playback and real-time monitoring using cpal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use thiserror::Error;
use tracing::{error, info};

#[derive(Debug, Error)]
pub enum AudioOutputError {
    #[error("No audio output devices found")]
    NoDevices,
    #[error("Failed to get default output config: {0}")]
    ConfigError(String),
    #[error("Failed to build output stream: {0}")]
    StreamError(String),
    #[error("Playback failed: {0}")]
    PlaybackError(String),
}

/// Handle to stop live monitoring
pub struct MonitorHandle {
    stop_flag: Arc<AtomicBool>,
    _stream: cpal::Stream,
}

impl MonitorHandle {
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

/// Handle for async playback completion
pub struct PlaybackHandle {
    done_rx: Receiver<Result<(), AudioOutputError>>,
    _stream: cpal::Stream,
}

impl PlaybackHandle {
    /// Wait for playback to complete
    pub fn wait(self) -> Result<(), AudioOutputError> {
        self.done_rx.recv().unwrap_or(Ok(()))
    }

    /// Check if playback is done (non-blocking)
    pub fn is_done(&self) -> bool {
        matches!(self.done_rx.try_recv(), Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected))
    }
}

pub struct AudioOutputService;

impl AudioOutputService {
    /// Play f32 samples through default output device (blocking)
    pub fn play_samples_blocking(samples: &[f32], sample_rate: u32) -> Result<(), AudioOutputError> {
        let handle = Self::play_samples(samples.to_vec(), sample_rate)?;
        handle.wait()
    }

    /// Play f32 samples through default output device (async, returns handle)
    pub fn play_samples(samples: Vec<f32>, sample_rate: u32) -> Result<PlaybackHandle, AudioOutputError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioOutputError::NoDevices)?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| AudioOutputError::ConfigError(e.to_string()))?;

        let device_sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels() as usize;

        // Calculate input audio stats
        let max_amplitude = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

        info!(
            device = %device.name().unwrap_or_default(),
            sample_rate = device_sample_rate,
            channels = channels,
            input_samples = samples.len(),
            max_amplitude = %format!("{:.4}", max_amplitude),
            rms = %format!("{:.4}", rms),
            "Starting audio playback"
        );

        // Resample if necessary
        let resampled = Self::resample_if_needed(&samples, sample_rate, device_sample_rate)?;

        // Normalize audio to prevent clipping but ensure audible level
        let resampled_max = resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let gain = if resampled_max > 0.001 { 0.8 / resampled_max } else { 1.0 };
        let normalized: Vec<f32> = resampled.iter().map(|s| s * gain).collect();

        info!(resampled_max = %format!("{:.4}", resampled_max), gain = %format!("{:.2}", gain), "Audio normalized");

        // Convert mono to stereo if needed
        let output_samples: Vec<f32> = match channels {
            1 => normalized,
            _ => normalized.iter().flat_map(|&s| std::iter::repeat(s).take(channels)).collect(),
        };

        let samples_arc = Arc::new(output_samples);
        let position = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let position_clone = position.clone();
        let samples_clone = samples_arc.clone();
        let total_samples = samples_arc.len();

        let (done_tx, done_rx) = channel();

        let config: StreamConfig = supported_config.into();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let pos = position_clone.load(Ordering::SeqCst);
                    for (i, sample) in data.iter_mut().enumerate() {
                        *sample = samples_clone.get(pos + i).copied().unwrap_or(0.0);
                    }
                    let new_pos = (pos + data.len()).min(total_samples);
                    position_clone.store(new_pos, Ordering::SeqCst);
                },
                move |err| error!("Playback stream error: {}", err),
                None,
            )
            .map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

        stream.play().map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

        info!(total_samples = total_samples, "Stream created, starting playback");

        // Spawn thread to monitor completion
        let position_monitor = position.clone();
        thread::spawn(move || {
            let duration_secs = total_samples as f64 / (device_sample_rate as f64 * channels as f64);
            let timeout = Duration::from_secs_f64(duration_secs + 0.5);
            let start = std::time::Instant::now();

            let mut last_logged_pct = 0;
            while start.elapsed() < timeout {
                let pos = position_monitor.load(Ordering::SeqCst);
                let pct = (pos * 100 / total_samples.max(1)) as u32;

                // Log progress every 25%
                if pct >= last_logged_pct + 25 {
                    info!(position = pos, total = total_samples, percent = pct, "Playback progress");
                    last_logged_pct = pct;
                }

                if pos >= total_samples {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }

            let final_pos = position_monitor.load(Ordering::SeqCst);
            info!(final_position = final_pos, total = total_samples, "Playback finished");
            let _ = done_tx.send(Ok(()));
        });

        Ok(PlaybackHandle { done_rx, _stream: stream })
    }

    /// Start live monitoring - plays audio from receiver through speakers
    /// input_sample_rate and input_channels describe the incoming audio format
    /// latency_ms controls the buffer size (lower = less delay, higher = more stable)
    pub fn start_monitor(
        audio_rx: Receiver<Vec<f32>>,
        input_sample_rate: u32,
        input_channels: u16,
        latency_ms: u32,
    ) -> Result<MonitorHandle, AudioOutputError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioOutputError::NoDevices)?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| AudioOutputError::ConfigError(e.to_string()))?;

        let output_sample_rate = supported_config.sample_rate().0;
        let output_channels = supported_config.channels() as usize;
        let input_channels = input_channels as usize;

        info!(
            device = %device.name().unwrap_or_default(),
            input_rate = input_sample_rate,
            output_rate = output_sample_rate,
            input_ch = input_channels,
            output_ch = output_channels,
            latency_ms,
            "Starting live audio monitor"
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Use a simple ring buffer approach with atomic indices for lock-free operation
        let buffer_size = output_sample_rate as usize * output_channels * 2; // 2 seconds max
        let buffer = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::<f32>::with_capacity(buffer_size)));
        let buffer_clone = buffer.clone();

        // Spawn thread to receive audio and feed buffer
        thread::spawn(move || {
            while !stop_flag_clone.load(Ordering::SeqCst) {
                let Ok(samples) = audio_rx.recv_timeout(Duration::from_millis(10)) else {
                    continue;
                };

                // Only resample if rates differ
                let resampled = match input_sample_rate == output_sample_rate {
                    true => samples,
                    false => match Self::resample_if_needed(&samples, input_sample_rate, output_sample_rate) {
                        Ok(r) => r,
                        Err(_) => continue,
                    },
                };

                // Handle channel conversion
                let output: Vec<f32> = match (input_channels, output_channels) {
                    (i, o) if i == o => resampled,
                    (1, o) => resampled.iter().flat_map(|&s| std::iter::repeat(s).take(o)).collect(),
                    (i, 1) => resampled.chunks(i).map(|c| c.iter().sum::<f32>() / i as f32).collect(),
                    (i, o) => {
                        // Convert to mono first, then expand
                        let mono: Vec<f32> = resampled.chunks(i).map(|c| c.iter().sum::<f32>() / i as f32).collect();
                        mono.iter().flat_map(|&s| std::iter::repeat(s).take(o)).collect()
                    }
                };

                let mut buf = buffer_clone.lock().unwrap();
                buf.extend(output);

                // Limit buffer to 2x latency to prevent runaway accumulation while allowing headroom
                let max_samples = (output_sample_rate as usize * output_channels * latency_ms as usize * 2) / 1000;
                while buf.len() > max_samples {
                    buf.pop_front();
                }
            }
        });

        let config: StreamConfig = supported_config.into();

        info!(latency_ms, "Output stream config");

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut buf = buffer.lock().unwrap();

                    for sample in data.iter_mut() {
                        *sample = buf.pop_front().unwrap_or(0.0);
                    }
                },
                move |err| error!("Monitor stream error: {}", err),
                None,
            )
            .map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

        stream.play().map_err(|e| AudioOutputError::StreamError(e.to_string()))?;

        Ok(MonitorHandle { stop_flag, _stream: stream })
    }

    /// Resample audio if sample rates differ
    fn resample_if_needed(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, AudioOutputError> {
        if from_rate == to_rate {
            return Ok(samples.to_vec());
        }

        use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, Resampler, WindowFunction};

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = SincFixedIn::<f32>::new(
            to_rate as f64 / from_rate as f64,
            2.0,
            params,
            samples.len(),
            1,
        ).map_err(|e| AudioOutputError::PlaybackError(format!("Resample init error: {}", e)))?;

        let input = vec![samples.to_vec()];
        let output = resampler
            .process(&input, None)
            .map_err(|e| AudioOutputError::PlaybackError(format!("Resample error: {}", e)))?;

        Ok(output.into_iter().flatten().collect())
    }
}
