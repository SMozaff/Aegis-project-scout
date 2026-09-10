use regex::Regex;
use serde::Deserialize;
use url::Url;

use crate::models::endpoint::PatternMatch;

const DEFAULT_PATTERNS: &str = include_str!("../../patterns/default_patterns.json");

#[derive(Debug, Clone, Deserialize)]
struct PatternDefinition {
    id: String,
    name: String,
    #[allow(dead_code)]
    description: String,
    #[serde(default = "default_pattern_category")]
    category: String,
    confidence: String,
    regex: String,
    extract_group: usize,
}

fn default_pattern_category() -> String {
    "endpoint".into()
}

struct CompiledPattern {
    definition: PatternDefinition,
    regex: Regex,
}

pub struct PatternAnalyzer {
    patterns: Vec<CompiledPattern>,
    secret_assignment: Regex,
    authorization_value: Regex,
}

impl PatternAnalyzer {
    pub fn load_default() -> Result<Self, String> {
        let definitions: Vec<PatternDefinition> = serde_json::from_str(DEFAULT_PATTERNS)
            .map_err(|e| format!("Default pattern file is invalid: {e}"))?;

        let mut patterns = Vec::with_capacity(definitions.len());
        for definition in definitions {
            let regex = Regex::new(&definition.regex)
                .map_err(|e| format!("Invalid regex in pattern '{}': {e}", definition.id))?;
            patterns.push(CompiledPattern { definition, regex });
        }

        let secret_assignment = Regex::new(
            r#"(?i)\b(token|api[_-]?key|secret|password)\b(\s*[:=]\s*[\"']?)[^\"'\s,;]{6,}"#,
        )
        .map_err(|e| e.to_string())?;
        let authorization_value = Regex::new(
            r#"(?i)(authorization\s*[:=]\s*[\"']?(?:bearer\s+)?)[A-Za-z0-9._~+/=-]{8,}"#,
        )
        .map_err(|e| e.to_string())?;

        Ok(Self {
            patterns,
            secret_assignment,
            authorization_value,
        })
    }

    pub fn analyze(&self, file_path: &str, content: &str) -> Vec<PatternMatch> {
        let mut matches = Vec::new();

        for (line_index, line) in content.lines().enumerate() {
            if line.len() > 8_192 {
                continue;
            }

            for compiled in &self.patterns {
                for captures in compiled.regex.captures_iter(line) {
                    let Some(raw_capture) = captures.get(compiled.definition.extract_group) else {
                        continue;
                    };

                    let normalized = trim_capture(raw_capture.as_str());
                    if normalized.len() < 2 {
                        continue;
                    }

                    let absolute_endpoint = normalize_health_endpoint(&normalized);
                    let captured = redact_url_query(&normalized);
                    let excerpt = self.redact_excerpt(line);

                    let confidence = if compiled.definition.category == "endpoint" {
                        deprioritize_confidence(&compiled.definition.confidence)
                    } else {
                        compiled.definition.confidence.clone()
                    };

                    matches.push(PatternMatch {
                        pattern_id: compiled.definition.id.clone(),
                        pattern_name: compiled.definition.name.clone(),
                        category: compiled.definition.category.clone(),
                        confidence,
                        file_path: file_path.to_string(),
                        line_number: line_index + 1,
                        excerpt,
                        captured,
                        absolute_endpoint,
                        verification: None,
                    });
                }
            }
        }

        matches
    }

    fn redact_excerpt(&self, line: &str) -> String {
        let first = self
            .secret_assignment
            .replace_all(line, "$1$2[REDACTED]")
            .to_string();
        let second = self
            .authorization_value
            .replace_all(&first, "$1[REDACTED]")
            .to_string();
        truncate_chars(second.trim(), 320)
    }
}

fn trim_capture(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c: char| matches!(c, '\'' | '"' | '`' | ',' | ';'))
        .trim_end_matches(|c: char| matches!(c, ')' | ']' | '}'))
        .to_string()
}

fn deprioritize_confidence(confidence: &str) -> String {
    match confidence {
        "high" => "medium".into(),
        "medium" => "low".into(),
        _ => confidence.into(),
    }
}

fn normalize_health_endpoint(value: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

fn redact_url_query(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return truncate_chars(value, 320);
    };
    if url.query().is_some() {
        url.set_query(Some("[REDACTED]"));
    }
    if url.fragment().is_some() {
        url.set_fragment(None);
    }
    truncate_chars(url.as_str(), 320)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    value.chars().take(limit).collect::<String>() + "…"
}
