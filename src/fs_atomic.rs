// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn temp_path_for(path: &Path) -> PathBuf {
    let tmp_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!("{ext}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    path.with_extension(tmp_extension)
}

pub(crate) fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = temp_path_for(path);
    fs::write(&tmp_path, bytes)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

pub(crate) fn write_string_atomically(path: &Path, content: &str) -> io::Result<()> {
    write_bytes_atomically(path, content.as_bytes())
}

pub(crate) fn write_toml_atomically<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let content = toml::to_string_pretty(value).map_err(io::Error::other)?;
    write_string_atomically(path, &content)
}

pub(crate) async fn write_bytes_atomically_async(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let tmp_path = temp_path_for(path);
    tokio::fs::write(&tmp_path, bytes).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_bytes_atomically_replaces_file_without_leaving_tmp() {
        let dir = tempdir().expect("create tempdir");
        let path = dir.path().join("sample.txt");

        write_bytes_atomically(&path, b"first").expect("write first");
        write_bytes_atomically(&path, b"second").expect("write second");

        assert_eq!(fs::read_to_string(&path).expect("read file"), "second");
        assert!(!path.with_extension("txt.tmp").exists());
    }
}
