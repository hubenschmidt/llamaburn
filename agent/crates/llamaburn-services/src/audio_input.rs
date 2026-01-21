//! Audio input service for microphone capture
//!
//! Provides device enumeration and audio capture using cpal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, StreamConfig};
use llamaburn_core::{AudioCaptureConfig, AudioDevice, AudioSampleFormat, DeviceType};
use thiserror::Error;
use tracing::{debug, error, info};

const TARGET_SAMPLE_RATE: u32 = 16000;

#[derive(Debug, Error)]
pub enum AudioInputError {
    #[error("No audio input devices found")]
    NoDevices,
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Failed to get default input config: {0}")]
    ConfigError(String),
    #[error("Failed to build input stream: {0}")]
    StreamError(String),
    #[error("Recording interrupted")]
    Interrupted,
    #[error("Resample error: {0}")]
    ResampleError(String),
}

/// Extension trait for cpal-specific AudioSampleFormat conversion
trait AudioSampleFormatExt {
    fn to_cpal(self) -> SampleFormat;
}

impl AudioSampleFormatExt for AudioSampleFormat {
    fn to_cpal(self) -> SampleFormat {
        match self {
            AudioSampleFormat::I16 => SampleFormat::I16,
            AudioSampleFormat::I24 => SampleFormat::I32, // cpal uses I32 for 24-bit
            AudioSampleFormat::F32 => SampleFormat::F32,
        }
    }
}

/// Handle to stop a running audio stream
/// Note: Not Send because cpal::Stream is not Send
pub struct StreamHandle {
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
    #[allow(dead_code)]
    stream: cpal::Stream, // Keep stream alive until handle is dropped
}

impl StreamHandle {
    /// Signal the stream to stop
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Wait for stream to finish
    pub fn join(mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

pub struct AudioInputService;

impl AudioInputService {
    /// Parse /proc/asound/cards for friendly card names
    /// Returns a map of card_id -> friendly_name
    fn parse_alsa_cards() -> std::collections::HashMap<String, String> {
        use std::collections::HashMap;
        use std::fs;

        let mut cards = HashMap::new();

        let Ok(content) = fs::read_to_string("/proc/asound/cards") else {
            return cards;
        };

        // Format:
        //  0 [Generic_1      ]: HDA-Intel - HD-Audio Generic
        //                       HD-Audio Generic at 0xfc600000 irq 72
        //  2 [R24            ]: USB-Audio - ZOOM R24
        for line in content.lines() {
            let line = line.trim();

            // Look for lines with [card_id]: ... - friendly_name
            let Some(bracket_start) = line.find('[') else { continue };
            let Some(bracket_end) = line.find(']') else { continue };
            let Some(dash_pos) = line.find(" - ") else { continue };

            let card_id = line[bracket_start + 1..bracket_end].trim().to_string();
            let friendly_name = line[dash_pos + 3..].trim().to_string();

            cards.insert(card_id, friendly_name);
        }

        cards
    }

    /// Extract card ID from ALSA device name
    /// e.g., "plughw:CARD=R24,DEV=0" -> Some("R24")
    fn extract_card_id(name: &str) -> Option<String> {
        // Look for CARD=xxx pattern
        let card_start = name.find("CARD=")?;
        let after_card = &name[card_start + 5..];

        // Find end (comma or end of string)
        let end = after_card.find(',').unwrap_or(after_card.len());
        Some(after_card[..end].to_string())
    }

    /// Determine device type from name prefix
    fn device_type_from_name(name: &str) -> DeviceType {
        if name.starts_with("plughw:") {
            return DeviceType::PluginHardware;
        }
        if name.starts_with("hw:") {
            return DeviceType::Hardware;
        }
        if name == "pulse" || name == "pipewire" {
            return DeviceType::PulseAudio;
        }
        if name == "default" || name.starts_with("sysdefault:") {
            return DeviceType::Default;
        }
        DeviceType::Other
    }

    /// List all available audio input devices
    pub fn list_devices() -> Result<Vec<AudioDevice>, AudioInputError> {
        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        // Get friendly card names from ALSA
        let card_names = Self::parse_alsa_cards();

        let devices: Vec<_> = host
            .input_devices()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?
            .filter_map(|device| {
                let name = device.name().ok()?;
                let config = device.default_input_config().ok()?;

                let card_id = Self::extract_card_id(&name);
                let card_name = card_id.as_ref().and_then(|id| card_names.get(id).cloned());
                let device_type = Self::device_type_from_name(&name);

                Some(AudioDevice {
                    id: name.clone(),
                    is_default: default_name.as_ref() == Some(&name),
                    name,
                    sample_rate: config.sample_rate().0,
                    channels: config.channels(),
                    card_id,
                    card_name,
                    device_type,
                })
            })
            .collect();

        if devices.is_empty() {
            return Err(AudioInputError::NoDevices);
        }

        info!(count = devices.len(), "Found audio input devices");
        for device in &devices {
            debug!(
                name = %device.name,
                sample_rate = device.sample_rate,
                channels = device.channels,
                default = device.is_default,
                card_name = ?device.card_name,
                "Audio device"
            );
        }

        Ok(devices)
    }

    /// Get device by ID
    fn get_device(device_id: &str) -> Result<Device, AudioInputError> {
        let host = cpal::default_host();

        // Try to find by name
        for device in host
            .input_devices()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?
        {
            if let Ok(name) = device.name() {
                if name == device_id {
                    return Ok(device);
                }
            }
        }

        // Fallback to default
        if device_id == "default" {
            return host
                .default_input_device()
                .ok_or(AudioInputError::NoDevices);
        }

        Err(AudioInputError::DeviceNotFound(device_id.to_string()))
    }

    /// Find best matching stream config for user preferences
    fn find_best_config(device: &Device, config: &AudioCaptureConfig) -> Result<cpal::SupportedStreamConfig, AudioInputError> {
        let supported_configs = device
            .supported_input_configs()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        let target_format = config.sample_format.to_cpal();
        let target_channels = config.channels;

        // Try to find exact match first
        let configs: Vec<_> = supported_configs.collect();

        // Score each config: lower is better
        let score_config = |c: &cpal::SupportedStreamConfigRange| -> u32 {
            let mut score = 0u32;

            // Format mismatch penalty
            score += (c.sample_format() != target_format) as u32 * 1000;

            // Channel mismatch penalty
            let ch_diff = (c.channels() as i32 - target_channels as i32).unsigned_abs();
            score += ch_diff * 100;

            // Sample rate: check if target is in range
            let min_rate = c.min_sample_rate().0;
            let max_rate = c.max_sample_rate().0;
            let rate_in_range = config.sample_rate >= min_rate && config.sample_rate <= max_rate;
            score += (!rate_in_range) as u32 * 500;

            score
        };

        let best = configs.into_iter()
            .min_by_key(|c| score_config(c))
            .ok_or_else(|| AudioInputError::ConfigError("No supported configs".to_string()))?;

        // Clamp sample rate to supported range
        let min_rate = best.min_sample_rate().0;
        let max_rate = best.max_sample_rate().0;
        let actual_rate = config.sample_rate.clamp(min_rate, max_rate);

        Ok(best.with_sample_rate(cpal::SampleRate(actual_rate)))
    }

    /// Capture audio for a fixed duration, return 16kHz mono f32 samples (for Whisper)
    pub fn capture(
        device_id: &str,
        duration_secs: u32,
    ) -> Result<Vec<f32>, AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        info!(
            device = %device.name().unwrap_or_default(),
            duration = duration_secs,
            sample_rate = config.sample_rate().0,
            channels = config.channels(),
            "Starting audio capture"
        );

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let expected_samples = (sample_rate * duration_secs) as usize * channels;

        let (tx, rx) = channel::<Vec<f32>>();
        let samples_collected = Arc::new(std::sync::Mutex::new(Vec::with_capacity(expected_samples)));
        let samples_for_callback = samples_collected.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config.into(), tx),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config.into(), tx),
            SampleFormat::I32 => Self::build_stream::<i32>(&device, &config.into(), tx),
            format => return Err(AudioInputError::ConfigError(format!("Unsupported format: {:?}", format))),
        }?;

        stream.play().map_err(|e| AudioInputError::StreamError(e.to_string()))?;

        // Collect samples for duration
        let deadline = std::time::Instant::now() + Duration::from_secs(duration_secs as u64);
        while std::time::Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                samples_for_callback
                    .lock()
                    .expect("audio sample mutex poisoned")
                    .extend(chunk);
            }
        }

        drop(stream);

        let raw_samples = std::mem::take(
            &mut *samples_collected.lock().expect("audio sample mutex poisoned"),
        );
        info!(samples = raw_samples.len(), "Capture complete");

        // Convert to mono and resample to 16kHz
        let mono = Self::to_mono(&raw_samples, channels);
        let resampled = Self::resample(&mono, sample_rate, TARGET_SAMPLE_RATE)?;

        Ok(resampled)
    }

    /// Capture audio with user-specified settings
    /// Returns raw f32 samples at the requested format (or closest supported)
    pub fn capture_with_config(
        device_id: &str,
        duration_secs: u32,
        audio_config: &AudioCaptureConfig,
    ) -> Result<(Vec<f32>, u32, u16), AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = Self::find_best_config(&device, audio_config)?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        info!(
            device = %device.name().unwrap_or_default(),
            duration = duration_secs,
            requested_rate = audio_config.sample_rate,
            actual_rate = sample_rate,
            requested_channels = audio_config.channels,
            actual_channels = channels,
            format = ?config.sample_format(),
            "Starting audio capture with config"
        );

        let expected_samples = (sample_rate * duration_secs) as usize * channels as usize;
        let (tx, rx) = channel::<Vec<f32>>();
        let samples_collected = Arc::new(std::sync::Mutex::new(Vec::with_capacity(expected_samples)));
        let samples_for_callback = samples_collected.clone();

        let sample_format = config.sample_format();
        let stream_config: StreamConfig = config.into();
        let stream = match sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &stream_config, tx),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &stream_config, tx),
            SampleFormat::I32 => Self::build_stream::<i32>(&device, &stream_config, tx),
            format => return Err(AudioInputError::ConfigError(format!("Unsupported format: {:?}", format))),
        }?;

        stream.play().map_err(|e| AudioInputError::StreamError(e.to_string()))?;

        let deadline = std::time::Instant::now() + Duration::from_secs(duration_secs as u64);
        while std::time::Instant::now() < deadline {
            let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            samples_for_callback
                .lock()
                .expect("audio sample mutex poisoned")
                .extend(chunk);
        }

        drop(stream);

        let raw_samples = std::mem::take(
            &mut *samples_collected.lock().expect("audio sample mutex poisoned"),
        );
        info!(samples = raw_samples.len(), sample_rate, channels, "Capture with config complete");

        Ok((raw_samples, sample_rate, channels))
    }

    /// Start streaming audio to a channel, returns handle to stop
    /// Audio is resampled to 16kHz mono for Whisper/analysis
    pub fn start_stream(
        device_id: &str,
        chunk_tx: Sender<Vec<f32>>,
    ) -> Result<StreamHandle, AudioInputError> {
        Self::start_stream_internal(device_id, chunk_tx, true)
    }

    /// Start raw streaming without resampling - for live monitoring
    /// Returns (StreamHandle, sample_rate, channels)
    pub fn start_stream_raw(
        device_id: &str,
        chunk_tx: Sender<Vec<f32>>,
    ) -> Result<(StreamHandle, u32, u16), AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        let handle = Self::start_stream_internal(device_id, chunk_tx, false)?;
        Ok((handle, sample_rate, channels))
    }

    fn start_stream_internal(
        device_id: &str,
        chunk_tx: Sender<Vec<f32>>,
        resample_to_16k: bool,
    ) -> Result<StreamHandle, AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        info!(
            device = %device.name().unwrap_or_default(),
            sample_rate = sample_rate,
            channels = channels,
            resample = resample_to_16k,
            "Starting audio stream"
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Chunk size: 50ms for low latency monitoring, 500ms for transcription
        let chunk_duration_ms = [50, 500][resample_to_16k as usize];
        let chunk_samples = (sample_rate * chunk_duration_ms / 1000) as usize * channels;

        let (internal_tx, internal_rx) = channel::<Vec<f32>>();

        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config.into(), internal_tx),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config.into(), internal_tx),
            SampleFormat::I32 => Self::build_stream::<i32>(&device, &config.into(), internal_tx),
            format => return Err(AudioInputError::ConfigError(format!("Unsupported format: {:?}", format))),
        }?;

        stream.play().map_err(|e| AudioInputError::StreamError(e.to_string()))?;

        // Spawn thread to process and forward audio chunks
        let thread_handle = thread::spawn(move || {
            let mut buffer: Vec<f32> = Vec::with_capacity(chunk_samples * 2);

            while !stop_flag_clone.load(Ordering::SeqCst) {
                let Ok(samples) = internal_rx.recv_timeout(Duration::from_millis(20)) else {
                    continue;
                };
                buffer.extend(samples);

                // Send chunks when we have enough samples
                while buffer.len() >= chunk_samples {
                    let chunk: Vec<f32> = buffer.drain(..chunk_samples).collect();

                    let output = match resample_to_16k {
                        true => {
                            let mono = Self::to_mono(&chunk, channels);
                            match Self::resample(&mono, sample_rate, TARGET_SAMPLE_RATE) {
                                Ok(r) => r,
                                Err(_) => continue,
                            }
                        }
                        false => chunk, // Pass through raw
                    };

                    if chunk_tx.send(output).is_err() {
                        return;
                    }
                }
            }

            info!("Audio stream stopped");
        });

        Ok(StreamHandle {
            stop_flag,
            thread_handle: Some(thread_handle),
            stream,
        })
    }

    fn build_stream<T>(
        device: &Device,
        config: &StreamConfig,
        tx: Sender<Vec<f32>>,
    ) -> Result<cpal::Stream, AudioInputError>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
        f32: FromSample<T>,
    {
        let err_fn = |err| error!("Audio stream error: {}", err);

        device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let samples: Vec<f32> = data.iter().map(|s| f32::from_sample_(*s)).collect();
                    let _ = tx.send(samples);
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioInputError::StreamError(e.to_string()))
    }

    /// Convert multi-channel audio to mono
    fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
        if channels == 1 {
            return samples.to_vec();
        }

        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    }

    /// Resample audio using rubato
    fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, AudioInputError> {
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
            1, // mono
        ).map_err(|e| AudioInputError::ResampleError(e.to_string()))?;

        let input = vec![samples.to_vec()];
        let output = resampler
            .process(&input, None)
            .map_err(|e| AudioInputError::ResampleError(e.to_string()))?;

        Ok(output.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        // This test will fail in CI but works locally
        let result = AudioInputService::list_devices();
        println!("Devices: {:?}", result);
    }
}
