//! Template generation logic.

use anyhow::{Context, Result};

use crate::metadata::template;

/// Generate a Challenge template.
pub async fn generate_challenge(name: &str, output_dir: &str) -> Result<()> {
    template::generate_challenge_template(name, output_dir)
        .context("Failed to generate challenge template")?;
    Ok(())
}

/// Generate a GameBox template.
pub async fn generate_gamebox(name: &str, output_dir: &str, basic: bool) -> Result<()> {
    if basic {
        template::generate_gamebox_basic_template(name, output_dir)
            .context("Failed to generate basic gamebox template")?;
    } else {
        template::generate_gamebox_template(name, output_dir)
            .context("Failed to generate gamebox template")?;
    }
    Ok(())
}
