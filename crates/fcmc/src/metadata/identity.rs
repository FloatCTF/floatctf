//! Shared artifact identity helpers (challenges/ vs gameboxes/).
//!
//! These are the single source of truth for safe-name derivation / validation,
//! package version validation, and canonical image reference construction.
//! Domain-specific wrappers (e.g. [`crate::metadata::gamebox`]) map the
//! `String` errors into their own error types.

/// Artifact namespace: `challenges/` vs `gameboxes/` in image names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    Challenge,
    GameBox,
}

impl ArtifactKind {
    /// Image namespace directory used in canonical image refs.
    pub fn dir(self) -> &'static str {
        match self {
            ArtifactKind::Challenge => "challenges",
            ArtifactKind::GameBox => "gameboxes",
        }
    }
}

/// Derive a Docker-repo-safe slug from a human `name`.
///
/// Rules:
/// - lowercase ASCII
/// - whitespace → `-`
/// - unsupported punctuation → `-`
/// - collapse repeated separators
/// - trim leading/trailing separators
/// - empty / non-ASCII-only → `None` (caller must require explicit `safe_name`)
pub fn derive_safe_name(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut last_sep = false;
    let mut saw_alnum = false;

    for c in name.chars() {
        let mapped = if c.is_ascii_alphanumeric() {
            saw_alnum = true;
            Some(c.to_ascii_lowercase())
        } else if c.is_ascii_whitespace() || c == '_' || c == '-' || c == '.' {
            Some('-')
        } else if c.is_ascii() {
            // other ASCII punctuation → separator
            Some('-')
        } else {
            // non-ASCII: drop (no pinyin); may leave empty
            None
        };

        if let Some(ch) = mapped {
            if ch == '-' {
                if !out.is_empty() && !last_sep {
                    out.push('-');
                    last_sep = true;
                }
            } else {
                out.push(ch);
                last_sep = false;
            }
        }
    }

    while out.ends_with('-') || out.ends_with('_') {
        out.pop();
    }

    if !saw_alnum || out.is_empty() {
        return None;
    }

    // Must start with [a-z0-9]
    if validate_safe_name(&out).is_ok() {
        Some(out)
    } else {
        None
    }
}

/// Validate an explicit `safe_name`: `^[a-z0-9][a-z0-9_-]*$`.
pub fn validate_safe_name(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err(format!("empty safe_name: '{s}'"));
    }
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return Err(format!("empty safe_name: '{s}'"));
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(format!(
            "invalid safe_name '{s}': must match ^[a-z0-9][a-z0-9_-]*$"
        ));
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
            return Err(format!(
                "invalid safe_name '{s}': must match ^[a-z0-9][a-z0-9_-]*$"
            ));
        }
    }
    Ok(())
}

/// Parse a package version as SemVer **without** build metadata (`+…` rejected).
/// Prerelease (`1.0.0-rc.1`) is allowed.
pub fn validate_version(version: &str) -> Result<semver::Version, String> {
    match semver::Version::parse(version) {
        Ok(v) => {
            // Double-check: semver crate accepts build metadata after `+`.
            if !v.build.is_empty() {
                Err(format!("build metadata is not allowed: '{version}'"))
            } else {
                Ok(v)
            }
        }
        Err(e) => Err(format!("invalid SemVer '{version}': {e}")),
    }
}

/// `{registry_prefix}/{challenges|gameboxes}/{safe_name}:{version}` — the ONLY
/// implementation of image naming.
///
/// `registry_prefix` comes from **platform config**, never from `meta.toml`.
/// CLI default when none is supplied: `"floatctf"`.
pub fn build_artifact_image_ref(
    kind: ArtifactKind,
    registry_prefix: &str,
    safe_name: &str,
    version: &str,
) -> String {
    let prefix = registry_prefix.trim_end_matches('/');
    format!("{prefix}/{}/{safe_name}:{version}", kind.dir())
}
