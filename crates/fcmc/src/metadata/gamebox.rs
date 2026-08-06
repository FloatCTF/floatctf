//! GameBox metadata — pure data structures and TOML parsing.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Resource limits for a GameBox container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// CPU limit in CPU millis (e.g. 1000 = 1 CPU).
    #[serde(default = "default_cpu_millis")]
    pub cpu_millis: i64,
    /// Memory limit in bytes (default 512MB).
    #[serde(default = "default_memory_bytes")]
    pub memory_bytes: i64,
    /// PID limit (default 100).
    #[serde(default = "default_pids_limit")]
    pub pids_limit: i64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            cpu_millis: default_cpu_millis(),
            memory_bytes: default_memory_bytes(),
            pids_limit: default_pids_limit(),
        }
    }
}

fn default_cpu_millis() -> i64 {
    1000
}
fn default_memory_bytes() -> i64 {
    536_870_912
} // 512 MB
fn default_pids_limit() -> i64 {
    100
}

/// Custom healthcheck configuration that overrides the image's built-in one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthcheckConfig {
    /// Command to run for health check (e.g. `["CMD-SHELL", "pgrep sshd"]`).
    pub test: Vec<String>,
    /// Interval between checks in seconds.
    #[serde(default = "default_healthcheck_interval")]
    pub interval_secs: u64,
    /// Timeout per check in seconds.
    #[serde(default = "default_healthcheck_timeout")]
    pub timeout_secs: u64,
    /// Number of retries before marking unhealthy.
    #[serde(default = "default_healthcheck_retries")]
    pub retries: u64,
    /// Start period before health checks begin.
    #[serde(default = "default_healthcheck_start_period")]
    pub start_period_secs: u64,
}

fn default_healthcheck_interval() -> u64 {
    30
}
fn default_healthcheck_timeout() -> u64 {
    10
}
fn default_healthcheck_retries() -> u64 {
    3
}
fn default_healthcheck_start_period() -> u64 {
    60
}

/// Judge check configuration for a GameBox template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeCheckConfig {
    /// Judge script filename (e.g. "check.py").
    pub script_name: String,
    /// Inline script content.
    pub script_content: String,
    /// JSON-encoded arguments template. `{target_ip}` is replaced at runtime.
    #[serde(default)]
    pub args_json: Option<String>,
    /// Per-template timeout override (seconds). Falls back to event default.
    pub timeout_secs: Option<i64>,
    /// Per-template retry interval override.
    pub retry_interval_secs: Option<i64>,
}

// ---------------------------------------------------------------------------
// Top-level GameBox config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameBoxMeta {
    pub name: String,
    pub author: String,
    pub category: String,
    pub description: String,
    pub gamebox: GameBoxConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameBoxConfig {
    /// The unprivileged user inside the container.
    pub username: String,
    /// Docker image reference (tag or digest).
    pub image_tag: String,

    /// Points awarded to the attacker per successful submission.
    #[serde(default = "default_score")]
    pub break_points: i64,
    /// Points awarded to the defender when judge check passes.
    #[serde(default = "default_score")]
    pub fix_points: i64,
    /// Points deducted from the defender when judge check fails.
    #[serde(default = "default_score")]
    pub down_points: i64,
    /// One-time bonus for the first successful attack on this template.
    #[serde(default = "default_first_bonus")]
    pub first_bonus: i64,

    /// Resource limits.
    #[serde(default)]
    pub resources: ResourceConfig,

    /// Healthcheck override.
    pub healthcheck: Option<HealthcheckConfig>,

    /// Judge configuration.
    pub judge: Option<JudgeCheckConfig>,
}

fn default_score() -> i64 {
    100
}
fn default_first_bonus() -> i64 {
    20
}

impl GameBoxMeta {
    /// Parse a GameBoxMeta from a TOML string.
    pub fn from_toml_str(toml: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_integer_score_fields() {
        let toml_str = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
image_tag = "test:latest"
break_points = 100
fix_points = 50
down_points = 200
first_bonus = 20
"#;
        let meta = GameBoxMeta::from_toml_str(toml_str).expect("parse new format");
        assert_eq!(meta.gamebox.break_points, 100);
        assert_eq!(meta.gamebox.fix_points, 50);
        assert_eq!(meta.gamebox.down_points, 200);
        assert_eq!(meta.gamebox.first_bonus, 20);
    }

    #[test]
    fn test_reject_fractional_score() {
        let toml_str = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
image_tag = "test:latest"
break_points = 100.5
fix_points = 50
down_points = 200
first_bonus = 20
"#;
        let result = GameBoxMeta::from_toml_str(toml_str);
        assert!(result.is_err(), "fractional scores must be rejected");
    }

    #[test]
    fn test_default_resource_limits() {
        let toml_str = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
image_tag = "test:latest"
break_points = 100
fix_points = 50
down_points = 200
first_bonus = 20
"#;
        let meta = GameBoxMeta::from_toml_str(toml_str).expect("parse with defaults");
        assert_eq!(meta.gamebox.resources.cpu_millis, 1000);
        assert_eq!(meta.gamebox.resources.memory_bytes, 536_870_912);
        assert_eq!(meta.gamebox.resources.pids_limit, 100);
    }

    #[test]
    fn test_parse_with_judge_config() {
        let toml_str = r#"
name = "test"
author = "test"
category = "Web"
description = "test"

[gamebox]
username = "ctf"
image_tag = "test:latest"
break_points = 100
fix_points = 50
down_points = 200
first_bonus = 20

[gamebox.judge]
script_name = "check.py"
script_content = "print('ok')"
args_json = '{"target": "{target_ip}"}'
timeout_secs = 15
"#;
        let meta = GameBoxMeta::from_toml_str(toml_str).expect("parse with judge");
        let judge = meta.gamebox.judge.expect("judge config present");
        assert_eq!(judge.script_name, "check.py");
        assert_eq!(judge.script_content, "print('ok')");
        assert_eq!(judge.timeout_secs, Some(15));
    }

    #[test]
    fn test_serialize_uses_new_field_names() {
        let config = GameBoxConfig {
            username: "ctf".into(),
            image_tag: "test:latest".into(),
            break_points: 100,
            fix_points: 50,
            down_points: 200,
            first_bonus: 20,
            resources: ResourceConfig::default(),
            healthcheck: None,
            judge: None,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(json.contains("\"break_points\""));
        assert!(!json.contains("\"break_point\""));
        assert!(json.contains("\"first_bonus\""));
        assert!(!json.contains("\"first_bouns\""));
    }
}
