use regex::{Regex, RegexSet};
use std::sync::OnceLock;

use crate::config::AppConfig;

pub fn rejection_reason(command: &str, config: &AppConfig) -> Option<&'static str> {
    if config
        .privacy
        .excluded_literals
        .iter()
        .filter(|literal| !literal.is_empty())
        .any(|literal| command.contains(literal))
    {
        return Some("configured literal exclusion");
    }
    if config
        .privacy
        .excluded_patterns
        .iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .any(|pattern| pattern.is_match(command))
    {
        return Some("configured pattern exclusion");
    }

    built_in_rejection_reason(command)
}

fn built_in_rejection_reason(command: &str) -> Option<&'static str> {
    let normalized = command.to_ascii_lowercase();

    if normalized.contains("-----begin ") && normalized.contains(" private key-----") {
        return Some("private key material");
    }
    if normalized.contains("authorization:") && normalized.contains("bearer ") {
        return Some("authorization credential");
    }
    if secret_patterns().is_match(command) {
        return Some("inline credential");
    }

    None
}

fn secret_patterns() -> &'static RegexSet {
    static PATTERNS: OnceLock<RegexSet> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        RegexSet::new([
            r#"(?i)(?:^|[\s;&|])(?:export\s+)?[a-z0-9_]*(?:api[_-]?key|token|secret|password|passwd|client[_-]?secret|access[_-]?key)[a-z0-9_]*\s*=\s*['\"]?[^\s'\"]+"#,
            r#"(?i)(?:^|\s)--(?:api[-_]?key|token|secret|password|passwd|client[-_]?secret)(?:\s+|=)\s*['\"]?[^\s'\"]+"#,
            r"(?i)(?:^|[^a-z0-9_])(?:sk-[a-z0-9_-]{8,}|github_pat_[a-z0-9_]+|gh[pousr]_[a-z0-9_]{8,}|xox[baprs]-[a-z0-9-]{8,})",
        ])
        .expect("built-in privacy patterns must compile")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_inline_secrets_without_keyword_false_positives() {
        let sensitive = [
            "export OPENAI_API_KEY='sk-live-example'",
            "curl --header 'Authorization: Bearer abc123' https://example.test",
            "docker login --password hunter2 registry.example.test",
            "mysql --password=correct-horse-battery-staple",
            "printf '%s' 'github_pat_example' | gh auth login --with-token",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
        ];
        for command in sensitive {
            assert!(
                rejection_reason(command, &AppConfig::default()).is_some(),
                "expected sensitive command: {command}"
            );
        }

        let safe = [
            "rg token src",
            "cargo test password_parser",
            "git log --grep api_key",
            "gh auth login --with-token",
            "op read op://team/registry/password",
        ];
        for command in safe {
            assert_eq!(
                rejection_reason(command, &AppConfig::default()),
                None,
                "unexpected rejection: {command}"
            );
        }
    }
}
