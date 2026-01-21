//! System-level types for hardware monitoring

/// GPU metrics collected from monitoring
#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub raw_output: String,
    pub connected: bool,
}
