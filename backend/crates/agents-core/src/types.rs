use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum WorkerType {
    Search,
    Email,
    General,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorDecision {
    pub worker_type: WorkerType,
    pub task_description: String,
    pub parameters: serde_json::Value,
    pub success_criteria: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorResult {
    pub passed: bool,
    pub score: u8,
    pub feedback: String,
    #[serde(default)]
    pub suggestions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    pub success: bool,
    pub output: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailParams {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    #[serde(default = "default_num_results")]
    pub num_results: u8,
}

fn default_num_results() -> u8 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontlineDecision {
    pub should_route: bool,
    pub response: String,
}
