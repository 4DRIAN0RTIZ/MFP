//! Filesystem locations under the user config directory (`~/.config/mfp`).

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Returns `<config_dir>/mfp`, failing with `missing_msg` if the platform
/// config directory cannot be resolved. Does not create anything.
fn mfp_dir(missing_msg: &'static str) -> Result<PathBuf> {
    Ok(dirs::config_dir().context(missing_msg)?.join("mfp"))
}

/// Path of the favorites file (`<config_dir>/mfp/favorites.json`).
///
/// Creates the `mfp` config directory if it does not exist.
pub fn favorites_path() -> Result<PathBuf> {
    let config_dir = mfp_dir("Failed to find config directory")?;

    fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

    Ok(config_dir.join("favorites.json"))
}

/// Path of the offline downloads directory (`<config_dir>/mfp/downloads`).
///
/// The directory is not created here; the caller creates it on demand.
pub fn downloads_dir() -> Result<PathBuf> {
    Ok(mfp_dir("No se pudo obtener el directorio de configuración")?.join("downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_dir_is_under_mfp_config_dir() {
        if let Some(base) = dirs::config_dir() {
            assert_eq!(downloads_dir().unwrap(), base.join("mfp").join("downloads"));
        }
    }
}
