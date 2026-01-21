use tracing::info;

use llamaburn_services::{AudioBenchmarkConfig, AudioBenchmarkResult, AudioMode, AudioSource, WhisperModel};
use llamaburn_services::{EffectDetectionService, WhisperService};

use super::{
    AudioAction, AudioBenchmarkEvent, AudioBenchmarkPanel, AudioTestState,
    LiveTranscriptionEvent, TranscriptionSegment,
};

impl AudioBenchmarkPanel {
    pub fn poll_audio_benchmark(&mut self) -> Vec<AudioAction> {
        let Some(rx) = self.audio_rx.take() else {
            return vec![];
        };

        let mut actions = Vec::new();
        let mut should_clear = false;
        let mut result_for_history: Option<AudioBenchmarkResult> = None;

        while let Ok(event) = rx.try_recv() {
            match event {
                AudioBenchmarkEvent::Progress(msg) => {
                    actions.push(AudioAction::AppendOutput(format!("{}\n", msg)));
                }
                AudioBenchmarkEvent::IterationComplete { iteration, metrics } => {
                    actions.push(AudioAction::SetProgress(format!("Iteration {}", iteration)));
                    actions.push(AudioAction::AppendOutput(format!(
                        "Run {}: RTF={:.3}x ({:.0}ms) | {} words\n",
                        iteration, metrics.real_time_factor, metrics.processing_time_ms, metrics.word_count
                    )));
                }
                AudioBenchmarkEvent::Done { metrics } => {
                    let summary = AudioBenchmarkResult::calculate_summary(&metrics);

                    actions.push(AudioAction::AppendOutput(format!(
                        "\nSummary\n-------\nAvg RTF: {:.3}x ({:.0}x real-time)\nAvg Time: {:.0}ms\nMin/Max RTF: {:.3}/{:.3}\n",
                        summary.avg_rtf, 1.0 / summary.avg_rtf, summary.avg_processing_ms, summary.min_rtf, summary.max_rtf,
                    )));

                    if let Some(first) = metrics.first() {
                        actions.push(AudioAction::AppendOutput(format!(
                            "\nTranscription ({} words):\n{}\n",
                            first.word_count, first.transcription
                        )));
                    }

                    let result = AudioBenchmarkResult {
                        config: AudioBenchmarkConfig {
                            audio_mode: AudioMode::Stt,
                            audio_source: AudioSource::File,
                            model_size: self.whisper_model,
                            audio_path: self.audio_file_path.clone().unwrap_or_default(),
                            language: None,
                            iterations: self.iterations,
                            warmup_runs: self.warmup,
                        },
                        metrics,
                        summary,
                    };

                    self.audio_result = Some(result.clone());
                    result_for_history = Some(result);
                    actions.push(AudioAction::SetProgress("Complete".to_string()));
                    self.running = false;
                    should_clear = true;
                }
                AudioBenchmarkEvent::Error(msg) => {
                    actions.push(AudioAction::AppendOutput(format!("\nError: {}\n", msg)));
                    actions.push(AudioAction::SetError(Some(msg)));
                    actions.push(AudioAction::SetProgress("Error".to_string()));
                    self.running = false;
                    should_clear = true;
                }
            }
        }

        if !should_clear {
            self.audio_rx = Some(rx);
        }

        // Build history entry after the loop to avoid borrow issues
        if let Some(result) = result_for_history {
            if let Some(entry) = self.build_audio_history_entry(&result) {
                actions.push(AudioAction::SaveHistory(entry));
            }
        }

        actions
    }

    pub fn start_audio_benchmark(&mut self) -> Vec<AudioAction> {
        let Some(audio_path) = self.audio_file_path.clone() else {
            return vec![];
        };
        let Some(model) = self.whisper_model else {
            return vec![];
        };

        info!("Starting audio benchmark: {:?}", audio_path);

        self.running = true;
        self.audio_result = None;

        let mut actions = vec![
            AudioAction::SetError(None),
            AudioAction::ClearOutput,
            AudioAction::SetProgress("Loading model...".to_string()),
        ];

        // Show config in live output
        let model_path = self.whisper_service.model_path(model);
        actions.push(AudioAction::AppendOutput(format!(
            "Audio Benchmark\n\
             ===============\n\
             Model: {} (~{}MB)\n\
             Path: {}\n\
             Audio: {}\n\
             Iterations: {}\n\
             Warmup: {}\n\n",
            model.label(),
            model.size_mb(),
            model_path.display(),
            audio_path.display(),
            self.iterations,
            self.warmup,
        )));

        // Create channel for async communication
        let (tx, rx) = std::sync::mpsc::channel();
        self.audio_rx = Some(rx);
        let iterations = self.iterations;
        let warmup = self.warmup;

        // Spawn background thread with stderr capture
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};

            // Create pipe to capture stderr
            let (stderr_read, stderr_write) = match os_pipe::pipe() {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(format!("Pipe error: {}", e)));
                    return;
                }
            };

            // Redirect stderr to our pipe
            let old_stderr = unsafe { libc::dup(2) };
            if old_stderr == -1 {
                let _ = tx.send(AudioBenchmarkEvent::Error("Failed to dup stderr".into()));
                return;
            }
            let dup2_result = unsafe {
                use std::os::fd::AsRawFd;
                libc::dup2(stderr_write.as_raw_fd(), 2)
            };
            if dup2_result == -1 {
                unsafe { libc::close(old_stderr) };
                let _ = tx.send(AudioBenchmarkEvent::Error(
                    "Failed to redirect stderr".into(),
                ));
                return;
            }
            drop(stderr_write); // Close write end in this thread

            // Spawn reader thread for stderr
            let tx_stderr = tx.clone();
            let reader_handle = std::thread::spawn(move || {
                let reader = BufReader::new(stderr_read);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    // Filter and send interesting lines
                    if line.contains("whisper_")
                        || line.contains("ggml_")
                        || line.contains("ROCm")
                        || line.contains("loading")
                        || line.contains("MB")
                        || line.contains("backend")
                        || line.starts_with("  Device")
                    {
                        let _ = tx_stderr.send(AudioBenchmarkEvent::Progress(line));
                    }
                }
            });

            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Loading model...".to_string(),
            ));

            let mut service = WhisperService::default();
            let result = service.run_benchmark(model, &audio_path, iterations, warmup, None);

            // Restore stderr
            unsafe {
                libc::dup2(old_stderr, 2);
                libc::close(old_stderr);
            }

            // Wait for reader to finish
            let _ = reader_handle.join();

            match result {
                Ok(metrics) => {
                    // Send iteration results
                    for (i, m) in metrics.iter().enumerate() {
                        let _ = tx.send(AudioBenchmarkEvent::IterationComplete {
                            iteration: (i + 1) as u32,
                            metrics: m.clone(),
                        });
                    }
                    let _ = tx.send(AudioBenchmarkEvent::Done { metrics });
                }
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(e.to_string()));
                }
            }
        });

        actions
    }

    pub fn start_capture_benchmark(&mut self) -> Vec<AudioAction> {
        use llamaburn_services::AudioInputService;

        let Some(device_id) = self.selected_device_id.clone() else {
            return vec![];
        };
        let Some(model) = self.whisper_model else {
            return vec![];
        };
        let duration = self.capture_duration_secs;

        info!(
            "Starting capture benchmark: device={}, duration={}s",
            device_id, duration
        );

        self.running = true;
        self.audio_result = None;

        let mut actions = vec![
            AudioAction::SetError(None),
            AudioAction::ClearOutput,
            AudioAction::SetProgress("Recording...".to_string()),
        ];

        // Show config in live output
        let model_path = self.whisper_service.model_path(model);
        actions.push(AudioAction::AppendOutput(format!(
            "Capture Benchmark\n\
             =================\n\
             Model: {} (~{}MB)\n\
             Path: {}\n\
             Device: {}\n\
             Duration: {}s\n\
             Iterations: {}\n\n\
             Recording audio...\n",
            model.label(),
            model.size_mb(),
            model_path.display(),
            device_id,
            duration,
            self.iterations,
        )));

        let (tx, rx) = std::sync::mpsc::channel();
        self.audio_rx = Some(rx);
        let iterations = self.iterations;

        std::thread::spawn(move || {
            // Step 1: Capture audio
            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Recording audio...".to_string(),
            ));

            let samples = match AudioInputService::capture(&device_id, duration) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AudioBenchmarkEvent::Error(format!("Capture error: {}", e)));
                    return;
                }
            };

            let _ = tx.send(AudioBenchmarkEvent::Progress(format!(
                "Captured {} samples ({:.1}s at 16kHz)",
                samples.len(),
                samples.len() as f64 / 16000.0
            )));

            // Step 2: Transcribe with benchmark iterations
            let _ = tx.send(AudioBenchmarkEvent::Progress(
                "Loading model...".to_string(),
            ));

            let service = WhisperService::default();
            let mut metrics_vec = Vec::new();

            for i in 0..iterations {
                let _ = tx.send(AudioBenchmarkEvent::Progress(format!(
                    "Iteration {} of {}...",
                    i + 1,
                    iterations
                )));

                match service.transcribe_samples(&samples) {
                    Ok((result, duration)) => {
                        let audio_duration_ms = (samples.len() as f64 / 16000.0) * 1000.0;
                        let processing_time_ms = duration.as_secs_f64() * 1000.0;
                        let real_time_factor = processing_time_ms / audio_duration_ms;
                        let word_count = result.text.split_whitespace().count() as u32;

                        let metrics = llamaburn_services::AudioBenchmarkMetrics {
                            real_time_factor,
                            processing_time_ms,
                            audio_duration_ms,
                            transcription: result.text.clone(),
                            word_count,
                        };

                        let _ = tx.send(AudioBenchmarkEvent::IterationComplete {
                            iteration: (i + 1) as u32,
                            metrics: metrics.clone(),
                        });

                        metrics_vec.push(metrics);
                    }
                    Err(e) => {
                        let _ = tx.send(AudioBenchmarkEvent::Error(format!(
                            "Transcription error: {}",
                            e
                        )));
                        return;
                    }
                }
            }

            let _ = tx.send(AudioBenchmarkEvent::Done {
                metrics: metrics_vec,
            });
        });

        actions
    }

    pub fn start_live_transcription_with_fx(&mut self, run_fx: bool) -> Vec<AudioAction> {
        use llamaburn_services::{AudioInputService, AudioOutputService};

        let Some(device_id) = self.selected_device_id.clone() else {
            return vec![AudioAction::SetError(Some("No audio device selected".to_string()))];
        };
        let Some(model) = self.whisper_model else {
            return vec![AudioAction::SetError(Some("No Whisper model selected".to_string()))];
        };

        // Check if monitoring was active - we'll re-enable it during recording
        let was_monitoring = matches!(self.audio_test_state, AudioTestState::Monitoring);

        // Stop existing monitor (we'll start our own)
        self.stop_live_monitor();

        let fx_tool = if run_fx {
            Some(self.selected_effect_tool)
        } else {
            None
        };

        info!(
            "Starting live transcription: device={}, fx={:?}, monitor={}",
            device_id, fx_tool, was_monitoring
        );

        // Reset state
        self.live_recording = true;
        self.running = true;
        self.effect_detection_running = run_fx;
        self.waveform_peaks.clear();
        self.transcription_segments.clear();
        self.recording_start = Some(std::time::Instant::now());

        let actions = vec![
            AudioAction::SetError(None),
            AudioAction::ClearOutput,
            AudioAction::SetProgress("Recording...".to_string()),
        ];

        // Create channels for processing
        let (audio_tx, audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let (event_tx, event_rx) = std::sync::mpsc::channel::<LiveTranscriptionEvent>();
        self.live_transcription_rx = Some(event_rx);

        // Start 16kHz audio stream for processing
        let stream_handle = match AudioInputService::start_stream(&device_id, audio_tx) {
            Ok(h) => h,
            Err(e) => {
                self.live_recording = false;
                self.running = false;
                return vec![AudioAction::SetError(Some(format!("Failed to start audio stream: {}", e)))];
            }
        };
        self.live_stream_handle = Some(stream_handle);

        // If monitoring was active, start a separate raw stream for monitoring output
        if was_monitoring {
            let (monitor_tx, monitor_rx) = std::sync::mpsc::channel::<Vec<f32>>();
            if let Ok((monitor_stream, sample_rate, channels)) =
                AudioInputService::start_stream_raw(&device_id, monitor_tx)
            {
                let latency = self.playback_latency_ms;
                let effect_chain = Some(self.effect_chain.clone());
                if let Ok(monitor_handle) = AudioOutputService::start_monitor_with_effects(
                    monitor_rx,
                    sample_rate,
                    channels,
                    latency,
                    effect_chain,
                ) {
                    // Store handles - we'll clean these up when recording stops
                    // Use a dummy field or just let them live until stop_recording
                    self.monitor_handle = Some(monitor_handle);
                    // Note: monitor_stream handle is dropped here but that's ok,
                    // the actual stream continues until monitor_rx is dropped
                    std::mem::forget(monitor_stream); // Keep stream alive
                }
            }
            self.audio_test_state = AudioTestState::Monitoring;
        }

        // Spawn processing thread with effect chain
        let event_tx_clone = event_tx.clone();
        let effect_chain = self.effect_chain.clone();
        std::thread::spawn(move || {
            let mut service = WhisperService::default();

            // Load the model
            if let Err(e) = service.load_model(model) {
                let _ = event_tx_clone.send(LiveTranscriptionEvent::Error(format!(
                    "Failed to load model: {}",
                    e
                )));
                return;
            }

            let mut accumulated_samples: Vec<f32> = Vec::new();
            let mut chunk_start_ms: u64 = 0;
            let max_chunk_samples = 16000 * 5; // 5 seconds max at 16kHz
            let min_chunk_samples = 16000 * 1; // 1 second min for VAD trigger

            // Set effect chain sample rate to match audio (16kHz)
            if let Ok(mut chain) = effect_chain.lock() {
                chain.set_sample_rate(16000.0);
            }

            // VAD parameters
            let silence_threshold = 0.01_f32; // RMS threshold for silence
            let silence_duration_samples = 16000 / 2; // 500ms of silence to trigger
            let mut consecutive_silence_samples = 0_usize;

            loop {
                // Receive audio chunk (with timeout to check for stop)
                let mut samples =
                    match audio_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok(s) => s,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };

                // Apply effects chain to audio before processing
                if let Ok(mut chain) = effect_chain.lock() {
                    chain.process(&mut samples);
                }

                // Compute peaks for waveform display (downsample to ~100 peaks per chunk)
                let peaks = Self::compute_waveform_peaks(&samples, 500);
                let _ = event_tx_clone.send(LiveTranscriptionEvent::AudioPeaks(peaks));

                // Calculate RMS for VAD
                let rms =
                    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();

                // Track silence duration
                if rms < silence_threshold {
                    consecutive_silence_samples += samples.len();
                } else {
                    consecutive_silence_samples = 0;
                }

                // Accumulate samples
                accumulated_samples.extend(samples);

                // Determine if we should process now:
                // 1. Max chunk size reached (5 seconds), OR
                // 2. VAD triggered: silence detected for 500ms AND we have at least 1 second of audio
                let max_reached = accumulated_samples.len() >= max_chunk_samples;
                let vad_triggered = consecutive_silence_samples >= silence_duration_samples
                    && accumulated_samples.len() >= min_chunk_samples;

                if !max_reached && !vad_triggered {
                    continue;
                }

                // Take all accumulated samples (up to max)
                let chunk_samples = accumulated_samples.len().min(max_chunk_samples);
                let chunk: Vec<f32> = accumulated_samples.drain(..chunk_samples).collect();
                let chunk_duration_ms = (chunk.len() as u64 * 1000) / 16000;
                let chunk_end_ms = chunk_start_ms + chunk_duration_ms;

                // Reset silence counter after processing
                consecutive_silence_samples = 0;

                // Set up streaming output channel
                let (stream_tx, stream_rx) = std::sync::mpsc::channel::<String>();
                let event_tx_stream = event_tx_clone.clone();

                // Spawn thread to forward streaming output
                std::thread::spawn(move || {
                    while let Ok(line) = stream_rx.recv() {
                        let _ = event_tx_stream.send(LiveTranscriptionEvent::StreamOutput(line));
                    }
                });

                // Transcribe chunk with streaming output
                let chunk_duration_secs = chunk_duration_ms as f64 / 1000.0;
                match service.transcribe_samples_streaming(&chunk, stream_tx) {
                    Ok((result, duration)) => {
                        let rtf = duration.as_secs_f64() / chunk_duration_secs.max(0.001);
                        let segment = TranscriptionSegment {
                            start_ms: chunk_start_ms,
                            end_ms: chunk_end_ms,
                            text: result.text,
                            rtf,
                        };
                        let _ = event_tx_clone.send(LiveTranscriptionEvent::Transcription(segment));
                    }
                    Err(e) => {
                        let _ = event_tx_clone.send(LiveTranscriptionEvent::Error(e.to_string()));
                    }
                }

                // Run FX detection on the same chunk if enabled
                if let Some(tool) = fx_tool {
                    // Save chunk to temp file for FX analysis
                    let temp_path = std::env::temp_dir().join("llamaburn_fx_chunk.wav");
                    if Self::save_samples_to_wav(&chunk, 16000, &temp_path).is_ok() {
                        let fx_service = EffectDetectionService::new(tool);
                        match fx_service.detect(&temp_path, None) {
                            Ok(result) => {
                                let _ =
                                    event_tx_clone.send(LiveTranscriptionEvent::FxDetection(result));
                            }
                            Err(e) => {
                                let _ = event_tx_clone.send(LiveTranscriptionEvent::Error(format!(
                                    "FX detection: {}",
                                    e
                                )));
                            }
                        }
                        let _ = std::fs::remove_file(&temp_path);
                    }
                }

                chunk_start_ms = chunk_end_ms;
            }

            let _ = event_tx_clone.send(LiveTranscriptionEvent::Stopped);
        });

        actions
    }

    pub fn stop_live_transcription(&mut self) {
        if let Some(handle) = self.live_stream_handle.take() {
            handle.stop();
        }
        self.live_recording = false;
        self.running = false;
        self.recording_start = None;
    }

    pub fn poll_live_transcription(&mut self) -> Vec<AudioAction> {
        let Some(rx) = &self.live_transcription_rx else {
            return vec![];
        };

        let mut actions = Vec::new();

        while let Ok(event) = rx.try_recv() {
            match event {
                LiveTranscriptionEvent::AudioPeaks(peaks) => {
                    self.waveform_peaks.extend(peaks);
                    // Keep last ~10 seconds worth of peaks (assuming ~100 peaks per 5s chunk)
                    while self.waveform_peaks.len() > 3000 {
                        self.waveform_peaks.pop_front();
                    }
                }
                LiveTranscriptionEvent::Transcription(segment) => {
                    self.transcription_segments.push(segment);
                }
                LiveTranscriptionEvent::StreamOutput(line) => {
                    actions.push(AudioAction::AppendOutput(format!("{}\n", line)));
                }
                LiveTranscriptionEvent::GpuMetrics(_metrics) => {
                    // TODO: Display GPU metrics
                }
                LiveTranscriptionEvent::FxDetection(result) => {
                    // Append FX detection results to live output
                    let mut output = String::from("\n--- Effect Detection ---\n");
                    output.push_str(&format!("Tool: {}\n", result.tool.label()));
                    output.push_str(&format!("Processing: {:.0}ms\n", result.processing_time_ms));
                    if result.effects.is_empty() {
                        output.push_str("No effects detected\n");
                    } else {
                        for effect in &result.effects {
                            output.push_str(&format!(
                                "  • {} ({:.0}%)\n",
                                effect.name,
                                effect.confidence * 100.0
                            ));
                        }
                    }
                    actions.push(AudioAction::AppendOutput(output));
                    self.effect_detection_result = Some(result);
                }
                LiveTranscriptionEvent::Error(e) => {
                    actions.push(AudioAction::SetError(Some(e)));
                }
                LiveTranscriptionEvent::Stopped => {
                    self.live_recording = false;
                    self.running = false;
                    self.effect_detection_running = false;
                }
            }
        }

        actions
    }

    pub fn unload_whisper_model(&mut self) {
        let Some(model) = self.whisper_model else {
            return;
        };

        info!("Unloading whisper model: {}", model.label());
        self.whisper_service.unload_model();
        self.whisper_model = None;
    }

    pub fn download_whisper_model(&mut self, model: WhisperModel) -> Vec<AudioAction> {
        let url = model.download_url();
        let path = self.whisper_service.model_path(model);

        info!("Opening download URL: {}", url);

        // Open URL in browser
        let _ = open::that(&url);

        vec![
            AudioAction::ClearOutput,
            AudioAction::AppendOutput(format!(
                "Download {} from:\n{}\n\nSave to:\n{}",
                model.label(),
                url,
                path.display()
            )),
        ]
    }

    /// Build history entry from benchmark result (without saving)
    pub fn build_audio_history_entry(&self, result: &AudioBenchmarkResult) -> Option<super::AudioHistoryEntry> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let model = self.whisper_model?;
        let model_id = format!("whisper-{}", model.label().to_lowercase());

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Some(super::AudioHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp,
            benchmark_type: llamaburn_services::BenchmarkType::Audio,
            audio_mode: llamaburn_services::AudioMode::Stt,
            model_id,
            config: result.config.clone(),
            summary: result.summary.clone(),
            metrics: result.metrics.clone(),
        })
    }

    pub fn compute_waveform_peaks(samples: &[f32], num_peaks: usize) -> Vec<(f32, f32)> {
        let samples_per_peak = (samples.len() / num_peaks).max(1);
        samples
            .chunks(samples_per_peak)
            .map(|chunk| {
                let min = chunk.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = chunk.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                (min, max)
            })
            .collect()
    }

    pub fn save_samples_to_wav(samples: &[f32], sample_rate: u32, path: &std::path::Path) -> Result<(), String> {
        use hound::{WavSpec, WavWriter};

        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV file: {}", e))?;

        for &sample in samples {
            writer
                .write_sample(sample)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

        Ok(())
    }
}
