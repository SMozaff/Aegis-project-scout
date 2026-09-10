
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    pub url: String,
    pub kind: EndpointKind,
    pub confidence: f64,
    pub evidence: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EndpointKind {
    ApiBase,
    OpenApi,
    Health,
    ModelEndpoint,
    GenerationEndpoint,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatch {
    pub pattern_id: String,
    pub pattern_name: String,
    pub category: String,
    pub confidence: String,
    pub file_path: String,
    pub line_number: usize,
    pub excerpt: String,
    pub captured: String,
    pub absolute_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointHealth {
    pub endpoint: String,
    pub reachable: bool,
    pub blocked: bool,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointMatch {
    pub endpoint: String,
    pub source_file: String,
    pub pattern_type: String,
    pub confidence: f64,
    pub context: String,
    pub is_placeholder: bool,
}
