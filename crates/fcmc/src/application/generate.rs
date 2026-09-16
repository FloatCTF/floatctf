//! 脚手架/模板生成用例。

use anyhow::{Context, Result};

use crate::metadata::template;
use crate::metadata::{SafeNameError, resolve_safe_name};

/// 生成前校验：content id（= 生成的目录名 = CLI `--name`）必须能解析出 safe_name，
/// 否则会产出一个立刻 invalid 的包。
///
/// `explicit_safe_name` 为空时从 `content_id` 派生；派生失败给出可执行的提示。
fn require_resolvable_safe_name(content_id: &str, explicit: Option<&str>) -> Result<String> {
    match resolve_safe_name(content_id, explicit) {
        Ok(safe) => Ok(safe),
        Err(SafeNameError::Underivable) => anyhow::bail!(
            "unable to derive safe_name from content id '{content_id}'; provide --safe-name"
        ),
        Err(SafeNameError::Invalid(raw)) => {
            anyhow::bail!("invalid safe_name '{raw}': must match ^[a-z0-9]+(?:[._-][a-z0-9]+)*$")
        }
    }
}

/// 生成 Challenge 模板。
pub async fn generate_challenge(
    name: &str,
    output_dir: &str,
    safe_name: Option<&str>,
) -> Result<()> {
    let safe = require_resolvable_safe_name(name, safe_name)?;
    template::generate_challenge_template(name, output_dir, safe_name, &safe)
        .context("Failed to generate challenge template")?;
    Ok(())
}

/// 生成 GameBox 模板。
pub async fn generate_gamebox(
    name: &str,
    output_dir: &str,
    basic: bool,
    safe_name: Option<&str>,
) -> Result<()> {
    if basic {
        // awd-base 模板内置固定身份（name/safe_name 都是 "awd-base"），
        // 不参与 content id 派生检查。
        template::generate_gamebox_basic_template(name, output_dir)
            .context("Failed to generate basic gamebox template")?;
    } else {
        let safe = require_resolvable_safe_name(name, safe_name)?;
        template::generate_gamebox_template(name, output_dir, safe_name, &safe)
            .context("Failed to generate gamebox template")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underivable_content_id_requires_explicit_safe_name() {
        let err = require_resolvable_safe_name("题目", None).unwrap_err();
        assert!(err.to_string().contains("provide --safe-name"));
        assert_eq!(
            require_resolvable_safe_name("题目", Some("challenge-001")).unwrap(),
            "challenge-001"
        );
        assert_eq!(
            require_resolvable_safe_name("Cirno's perfect math class", None).unwrap(),
            "cirnos-perfect-math-class"
        );
        assert!(require_resolvable_safe_name("x", Some("Foo Bar")).is_err());
    }
}
