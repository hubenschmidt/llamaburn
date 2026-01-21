//! Whisper Speech-to-Text service
//!
//! Requires the `whisper` feature to be enabled.
//! For ROCm GPU support, use the `whisper-gpu` feature.
//!
//! Install system dependencies: `sudo apt install cmake clang`

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::time::Instant;

use llamaburn_core::{
    AudioBenchmarkMetrics, Segment, TranscriptionResult, WhisperEvent, WhisperModel,
};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum WhisperError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Failed to load model: {0}")]
    ModelLoadError(String),
    #[error("Failed to load audio: {0}")]
    AudioLoadError(String),
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),
    #[error("Audio format not supported: {0}")]
    UnsupportedFormat(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Create default FullParams for whisper transcription
fn default_params(streaming: bool) -> whisper_rs::FullParams<'static, 'static> {
    let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_progress(streaming);
    params.set_print_realtime(streaming);
    params.set_print_timestamps(streaming);
    if streaming {
        params.set_print_special(true);
    }
    params
}

/// Extract segments from whisper state after transcription
fn extract_segments(state: &whisper_rs::WhisperState) -> Result<(String, Vec<Segment>), WhisperError> {
    let num_segments = state.full_n_segments();
    (0..num_segments)
        .try_fold((String::new(), Vec::new()), |(mut text, mut segs), i| {
            let Some(seg) = state.get_segment(i) else {
                return Ok((text, segs));
            };
            let seg_text = seg
                .to_str()
                .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?
                .to_string();
            text.push_str(&seg_text);
            segs.push(Segment {
                start_ms: seg.start_timestamp() * 10,
                end_ms: seg.end_timestamp() * 10,
                text: seg_text,
            });
            Ok((text, segs))
        })
}

pub struct WhisperService {
    model_dir: PathBuf,
    current_model: Option<WhisperModel>,
    context: Option<whisper_rs::WhisperContext>,
}

impl WhisperService {
    pub fn new(model_dir: &Path) -> Self {
        std::fs::create_dir_all(model_dir).ok();
        Self {
            model_dir: model_dir.to_path_buf(),
            current_model: None,
            context: None,
        }
    }

    pub fn default_model_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("llamaburn")
            .join("whisper")
    }

    pub fn model_path(&self, model: WhisperModel) -> PathBuf {
        self.model_dir.join(model.filename())
    }

    pub fn is_model_downloaded(&self, model: WhisperModel) -> bool {
        self.model_path(model).exists()
    }

    pub fn load_model(&mut self, model: WhisperModel) -> Result<Duration, WhisperError> {
        let path = self.model_path(model);

        if !path.exists() {
            return Err(WhisperError::ModelNotFound(format!(
                "Model {} not found at {}. Download from {}",
                model.label(),
                path.display(),
                model.download_url()
            )));
        }

        info!("Loading Whisper model: {} from {}", model.label(), path.display());
        let start = Instant::now();

        let ctx = whisper_rs::WhisperContext::new_with_params(
            path.to_str().ok_or_else(|| WhisperError::ModelLoadError("Invalid path".into()))?,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|e| WhisperError::ModelLoadError(e.to_string()))?;

        let elapsed = start.elapsed();
        info!("Model loaded in {:?}", elapsed);

        self.context = Some(ctx);
        self.current_model = Some(model);

        Ok(elapsed)
    }

    pub fn current_model(&self) -> Option<WhisperModel> {
        self.current_model
    }

    /// Unload the currently loaded model to free memory
    pub fn unload_model(&mut self) {
        self.current_model = None;
        {
            self.context = None;
        }
    }

    pub fn transcribe(&self, audio_path: &Path) -> Result<TranscriptionResult, WhisperError> {
        let (result, _) = self.transcribe_with_timing(audio_path)?;
        Ok(result)
    }

    pub fn transcribe_with_timing(
        &self,
        audio_path: &Path,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        let audio_data = self.load_audio(audio_path)?;
        debug!("Audio loaded: {} samples", audio_data.len());

        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(default_params(false), &audio_data)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();
        let (full_text, segments) = extract_segments(&state)?;

        Ok((
            TranscriptionResult {
                text: full_text.trim().to_string(),
                segments,
                language: "en".to_string(),
            },
            elapsed,
        ))
    }

    /// Transcribe from raw 16kHz mono f32 samples (for captured microphone audio)
    pub fn transcribe_samples(&self, samples: &[f32]) -> Result<(TranscriptionResult, Duration), WhisperError> {
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        debug!("Transcribing {} samples", samples.len());

        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(default_params(false), samples)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();
        let (full_text, segments) = extract_segments(&state)?;

        Ok((
            TranscriptionResult {
                text: full_text.trim().to_string(),
                segments,
                language: "en".to_string(),
            },
            elapsed,
        ))
    }

    /// Transcribe with verbose output and streaming segment callback
    pub fn transcribe_samples_streaming(
        &self,
        samples: &[f32],
        segment_tx: std::sync::mpsc::Sender<String>,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        debug!("Transcribing {} samples (streaming)", samples.len());

        let mut params = default_params(true);
        params.set_segment_callback_safe_lossy(move |segment: whisper_rs::SegmentCallbackData| {
            let text = segment.text.trim();
            if text.is_empty() {
                return;
            }
            let _ = segment_tx.send(format!(
                "[{:.2}s → {:.2}s] {}",
                segment.start_timestamp as f64 / 100.0,
                segment.end_timestamp as f64 / 100.0,
                text
            ));
        });

        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(params, samples)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();
        let (full_text, segments) = extract_segments(&state)?;

        Ok((
            TranscriptionResult {
                text: full_text.trim().to_string(),
                segments,
                language: "en".to_string(),
            },
            elapsed,
        ))
    }

    fn load_audio(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "wav" => self.load_wav(path),
            "mp3" | "flac" | "m4a" | "aac" | "ogg" => self.load_with_symphonia(path),
            _ => Err(WhisperError::UnsupportedFormat(ext)),
        }
    }

    fn load_wav(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let spec = reader.spec();
        debug!(
            "WAV: {} Hz, {} channels, {:?}",
            spec.sample_rate, spec.channels, spec.sample_format
        );

        let samples = self.read_wav_samples(reader, &spec)?;
        let mono = self.to_mono(&samples, spec.channels);

        if spec.sample_rate == 16000 {
            return Ok(mono);
        }

        self.resample(&mono, spec.sample_rate, 16000)
    }

    fn read_wav_samples(
        &self,
        reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
        spec: &hound::WavSpec,
    ) -> Result<Vec<f32>, WhisperError> {
        if spec.sample_format == hound::SampleFormat::Float {
            return Ok(reader.into_samples::<f32>().filter_map(Result::ok).collect());
        }

        let bits = spec.bits_per_sample;
        let max = (1 << (bits - 1)) as f32;
        Ok(reader
            .into_samples::<i32>()
            .filter_map(Result::ok)
            .map(|s| s as f32 / max)
            .collect())
    }

    fn to_mono(&self, samples: &[f32], channels: u16) -> Vec<f32> {
        if channels == 1 {
            return samples.to_vec();
        }

        samples
            .chunks(channels as usize)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    }

    fn load_with_symphonia(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let file = std::fs::File::open(path)
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| WhisperError::AudioLoadError("No audio track found".into()))?;

        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let track_id = track.id;
        let samples = self.decode_all_packets(&mut format, &mut decoder, track_id)?;
        let mono = self.to_mono(&samples, channels as u16);

        if sample_rate == 16000 {
            return Ok(mono);
        }

        self.resample(&mono, sample_rate, 16000)
    }

    fn decode_all_packets(
        &self,
        format: &mut Box<dyn symphonia::core::formats::FormatReader>,
        decoder: &mut Box<dyn symphonia::core::codecs::Decoder>,
        track_id: u32,
    ) -> Result<Vec<f32>, WhisperError> {
        use symphonia::core::audio::SampleBuffer;

        let mut samples: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(samples),
                Err(e) => {
                    warn!("Error reading packet: {}", e);
                    return Ok(samples);
                }
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(e) => {
                    warn!("Error decoding packet: {}", e);
                    continue;
                }
            };

            let spec = *decoded.spec();
            let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend(sample_buf.samples());
        }
    }

    fn resample(&self, input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, WhisperError> {
        use rubato::{FftFixedIn, Resampler};

        let chunk_size = 1024;

        let mut resampler = FftFixedIn::<f32>::new(from_rate as usize, to_rate as usize, chunk_size, 2, 1)
            .map_err(|e| WhisperError::AudioLoadError(format!("Resampler init failed: {}", e)))?;

        let mut output = Vec::new();
        let mut pos = 0;

        while pos < input.len() {
            let end = (pos + chunk_size).min(input.len());
            let chunk = &input[pos..end];

            // Pad if needed
            let padded: Vec<f32> = if chunk.len() < chunk_size {
                let mut p = chunk.to_vec();
                p.resize(chunk_size, 0.0);
                p
            } else {
                chunk.to_vec()
            };

            let resampled = resampler
                .process(&[padded], None)
                .map_err(|e| WhisperError::AudioLoadError(format!("Resample failed: {}", e)))?;

            output.extend(&resampled[0]);
            pos += chunk_size;
        }

        debug!(
            "Resampled {} samples at {}Hz to {} samples at {}Hz",
            input.len(), from_rate, output.len(), to_rate
        );

        Ok(output)
    }

    pub fn run_benchmark(
        &mut self,
        model: WhisperModel,
        audio_path: &Path,
        iterations: u32,
        warmup: u32,
        tx: Option<Sender<WhisperEvent>>,
    ) -> Result<Vec<AudioBenchmarkMetrics>, WhisperError> {
        let send = |event: WhisperEvent| {
            if let Some(ref tx) = tx {
                let _ = tx.send(event);
            }
        };

        // Load model if needed
        if self.current_model != Some(model) {
            send(WhisperEvent::LoadingModel { model });
            let load_time = self.load_model(model)?;
            send(WhisperEvent::ModelLoaded {
                load_time_ms: load_time.as_millis() as u64,
            });
        }

        // Load audio once
        send(WhisperEvent::LoadingAudio {
            path: audio_path.to_path_buf(),
        });
        let audio_data = self.load_audio(audio_path)?;
        let audio_duration_ms = (audio_data.len() as f64 / 16.0) as f64; // 16kHz = 16 samples/ms
        send(WhisperEvent::AudioLoaded {
            duration_ms: audio_duration_ms as u64,
        });

        // Warmup runs
        for i in 0..warmup {
            debug!("Warmup run {}/{}", i + 1, warmup);
            let _ = self.transcribe(audio_path)?;
        }

        // Benchmark runs
        let mut metrics = Vec::with_capacity(iterations as usize);

        for i in 0..iterations {
            send(WhisperEvent::Transcribing);

            let (result, duration) = self.transcribe_with_timing(audio_path)?;
            let processing_ms = duration.as_secs_f64() * 1000.0;
            let rtf = processing_ms / audio_duration_ms;

            send(WhisperEvent::TranscriptionComplete {
                result: result.clone(),
                processing_ms: processing_ms as u64,
            });

            metrics.push(AudioBenchmarkMetrics {
                real_time_factor: rtf,
                processing_time_ms: processing_ms,
                audio_duration_ms,
                transcription: result.text,
                word_count: result.segments.len() as u32,
            });

            debug!(
                "Iteration {}/{}: RTF={:.3}, time={:.1}ms",
                i + 1, iterations, rtf, processing_ms
            );
        }

        Ok(metrics)
    }
}

impl Default for WhisperService {
    fn default() -> Self {
        Self::new(&Self::default_model_dir())
    }
}

pub fn get_audio_duration_ms(path: &Path) -> Result<f64, WhisperError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "wav" {
        return get_wav_duration_ms(path);
    }

    get_symphonia_duration_ms(path, &ext)
}

fn get_wav_duration_ms(path: &Path) -> Result<f64, WhisperError> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let spec = reader.spec();
    let samples = reader.len() as f64;
    let duration_sec = samples / spec.channels as f64 / spec.sample_rate as f64;

    Ok(duration_sec * 1000.0)
}

fn get_symphonia_duration_ms(path: &Path, ext: &str) -> Result<f64, WhisperError> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let track = probed
        .format
        .default_track()
        .ok_or_else(|| WhisperError::AudioLoadError("No audio track".into()))?;

    let n_frames = track
        .codec_params
        .n_frames
        .ok_or_else(|| WhisperError::AudioLoadError("Cannot determine duration".into()))?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let duration_sec = n_frames as f64 / sample_rate as f64;

    Ok(duration_sec * 1000.0)
}
