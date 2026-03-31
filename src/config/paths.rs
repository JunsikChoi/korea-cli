//! Platform-specific paths for korea-cli data.

use std::path::PathBuf;

pub fn config_dir() -> anyhow::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "korea-cli")
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;
    let path = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn config_file() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn bundle_override_file() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("bundle.zstd"))
}
