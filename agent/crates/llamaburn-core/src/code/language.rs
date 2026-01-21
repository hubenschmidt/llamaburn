use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Python,
    JavaScript,
    Rust,
    Go,
}

impl Language {
    pub fn label(&self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::Rust => "Rust",
            Language::Go => "Go",
        }
    }

    pub fn file_extension(&self) -> &'static str {
        match self {
            Language::Python => "py",
            Language::JavaScript => "js",
            Language::Rust => "rs",
            Language::Go => "go",
        }
    }

    pub fn all() -> &'static [Language] {
        &[
            Language::Python,
            Language::JavaScript,
            Language::Rust,
            Language::Go,
        ]
    }
}
