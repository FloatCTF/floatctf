use anyhow::{Result, bail};

use crate::{command, validation};

pub(crate) async fn flush(cidr: &str) -> Result<()> {
    validation::cidr(cidr)?;
    let out = command::run("conntrack", &["-D", "-s", cidr]).await?;
    if out.code != 0 && out.code != 1 {
        bail!("conntrack flush failed (exit {}): {}", out.code, out.stderr);
    }
    Ok(())
}
