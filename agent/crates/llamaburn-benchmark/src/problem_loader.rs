use std::path::Path;

use llamaburn_core::ProblemSet;

#[derive(Debug, thiserror::Error)]
pub enum ProblemLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn load_problem_set(path: &Path) -> Result<ProblemSet, ProblemLoaderError> {
    let content = std::fs::read_to_string(path)?;
    let problem_set: ProblemSet = serde_json::from_str(&content)?;
    Ok(problem_set)
}

pub fn load_all_problem_sets(dir: &Path) -> Result<Vec<ProblemSet>, ProblemLoaderError> {
    let mut sets = Vec::new();
    let entries = std::fs::read_dir(dir)?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(set) = load_problem_set(&path) {
                sets.push(set);
            }
        }
    }

    Ok(sets)
}
