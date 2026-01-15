use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, instrument, warn};

#[derive(Error, Debug)]
pub enum GpuMonitorError {
    #[error("Failed to execute rocm-smi: {0}")]
    ExecutionFailed(#[from] std::io::Error),
    #[error("rocm-smi not found - is ROCm installed?")]
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub raw_output: String,
    pub connected: bool,
}

pub struct GpuMonitor {
    poll_interval: Duration,
}

impl GpuMonitor {
    pub fn new(poll_interval: Duration) -> Self {
        Self { poll_interval }
    }

    pub fn default_interval() -> Self {
        Self::new(Duration::from_secs(1))
    }

    #[instrument(skip(self))]
    pub fn get_metrics(&self) -> Result<GpuMetrics, GpuMonitorError> {
        debug!("Fetching GPU metrics via rocm-smi");

        let output = Command::new("rocm-smi")
            .arg("--showmeminfo")
            .arg("vram")
            .arg("--showtemp")
            .arg("--showuse")
            .output();

        match output {
            Ok(out) => {
                debug!("rocm-smi executed successfully");
                Ok(GpuMetrics {
                    raw_output: String::from_utf8_lossy(&out.stdout).to_string(),
                    connected: true,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                error!("rocm-smi not found");
                Err(GpuMonitorError::NotFound)
            }
            Err(e) => {
                error!("rocm-smi execution failed: {}", e);
                Err(GpuMonitorError::ExecutionFailed(e))
            }
        }
    }

    /// Subscribe to GPU metrics updates
    /// Returns a receiver that will receive metrics at the configured poll interval
    #[instrument(skip(self), fields(interval_ms = self.poll_interval.as_millis()))]
    pub fn subscribe(&self) -> Receiver<GpuMetrics> {
        info!("Starting GPU monitor subscription");
        let (tx, rx) = channel();
        let interval = self.poll_interval;

        thread::spawn(move || {
            let mut connected_logged = false;

            loop {
                let metrics = match Command::new("rocm-smi")
                    .arg("--showmeminfo")
                    .arg("vram")
                    .arg("--showtemp")
                    .arg("--showuse")
                    .output()
                {
                    Ok(out) => {
                        if !connected_logged {
                            info!("GPU monitor connected");
                            connected_logged = true;
                        }
                        GpuMetrics {
                            raw_output: String::from_utf8_lossy(&out.stdout).to_string(),
                            connected: true,
                        }
                    }
                    Err(e) => {
                        if connected_logged {
                            warn!("GPU monitor disconnected: {}", e);
                            connected_logged = false;
                        }
                        GpuMetrics {
                            raw_output: String::new(),
                            connected: false,
                        }
                    }
                };

                if tx.send(metrics).is_err() {
                    debug!("GPU monitor channel closed, stopping");
                    break;
                }

                thread::sleep(interval);
            }
        });

        rx
    }
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::default_interval()
    }
}
