//! Utility functions for code benchmark panel

use std::path::PathBuf;

use llamaburn_services::load_all_problem_sets;
use llamaburn_services::ProblemSet;

/// Temperature bucket values
pub const TEMPERATURE_BUCKETS: &[f32] = &[0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.4];

/// Max tokens bucket values
pub const MAX_TOKENS_BUCKETS: &[u32] = &[512, 1024, 2048, 4096, 8192];

/// Check if an error is a harness/infrastructure error (vs LLM code failure)
pub fn is_harness_error(error: &Option<String>) -> bool {
    let Some(e) = error else { return false };
    e.contains("FromArgMut") || e.contains("FromArg") || e.contains("Arg::as_mut_arg")
}

/// Format duration in milliseconds as human-readable ETA
pub fn format_duration_eta(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        return format!("ETA: {}h {}m", hours, mins);
    }
    if mins > 0 {
        return format!("ETA: {}m {}s", mins, secs);
    }
    format!("ETA: {}s", secs)
}

/// Format temperature dropdown label
pub fn format_temp_label(temps: &[f32]) -> String {
    match temps.len() {
        0 => "Temp: None".to_string(),
        1 => format!("Temp: {:.1}", temps[0]),
        n if n == TEMPERATURE_BUCKETS.len() => format!("Temp: All ({})", n),
        n => format!("Temp: {} values", n),
    }
}

/// Format max tokens dropdown label
pub fn format_tokens_label(tokens: &[u32]) -> String {
    match tokens.len() {
        0 => "Tokens: None".to_string(),
        1 => format!("Tokens: {}", tokens[0]),
        n if n == MAX_TOKENS_BUCKETS.len() => format!("Tokens: All ({})", n),
        n => format!("Tokens: {} values", n),
    }
}

pub fn load_problem_sets_from_disk() -> Vec<ProblemSet> {
    let Some(dir) = find_problems_dir() else {
        tracing::warn!("Problems directory not found, using empty set");
        return Vec::new();
    };

    load_all_problem_sets(&dir).unwrap_or_else(|e| {
        tracing::error!("Failed to load problem sets: {}", e);
        Vec::new()
    })
}

fn find_problems_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("problems"),
        PathBuf::from("../problems"),
        PathBuf::from("../../problems"),
    ];

    if let Some(found) = candidates.into_iter().find(|p| p.is_dir()) {
        return Some(found);
    }

    let exe_path = std::env::current_exe().ok()?;
    let from_exe = exe_path.parent()?.join("problems");
    from_exe.is_dir().then_some(from_exe)
}
