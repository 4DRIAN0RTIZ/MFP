use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CHUNK_SIZE: usize = 32 * 1024; // Chunk size: 32 KB
const MIB: u64 = 1024 * 1024;

/// The MiB count to report when `downloaded` has crossed a MiB boundary beyond
/// `last_reported_mib`, or `None` when no new boundary was crossed.
///
/// Reads arrive in arbitrary chunk sizes, so an exact `% MIB == 0` check would
/// almost never fire.
fn next_progress_mib(downloaded: u64, last_reported_mib: u64) -> Option<u64> {
    let mib = downloaded / MIB;
    (mib > last_reported_mib).then_some(mib)
}

/// Progress notifications emitted by [`Downloader::download_episode`].
///
/// The downloader never prints; callers decide how (or whether) to render.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadEvent {
    /// The file already exists locally; no download happens.
    AlreadyDownloaded { filename: String },
    /// A download is about to start.
    Started { title: String },
    /// Emitted each time the running total crosses a new MiB boundary, only
    /// when the total size is known.
    Progress { downloaded: u64, total: u64 },
    /// The transfer finished; `downloaded` is the total bytes received.
    Finished { downloaded: u64 },
}

/// Result of [`Downloader::delete_episode`].
#[derive(Debug, Clone, PartialEq)]
pub enum DeleteOutcome {
    /// The file existed and was removed.
    Deleted { filename: String },
    /// There was nothing to delete.
    NotDownloaded,
}

pub struct Downloader {
    download_dir: PathBuf,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let download_dir = crate::config::downloads_dir()?;

        fs::create_dir_all(&download_dir)?;

        Ok(Downloader { download_dir })
    }

    /// Downloads an episode, reporting progress through `on_event`.
    pub fn download_episode(
        &self,
        title: &str,
        url: &str,
        mut on_event: impl FnMut(DownloadEvent),
    ) -> Result<PathBuf> {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);

        if file_path.exists() {
            on_event(DownloadEvent::AlreadyDownloaded { filename });
            return Ok(file_path);
        }

        on_event(DownloadEvent::Started {
            title: title.to_string(),
        });

        let mut response =
            reqwest::blocking::get(url).context("No se pudo conectar al servidor")?;

        if !response.status().is_success() {
            anyhow::bail!("Error HTTP: {}", response.status());
        }

        let total_size = response.content_length();

        let temp_path = file_path.with_extension("tmp");
        let mut file = File::create(&temp_path).context("No se pudo crear el archivo")?;

        let mut downloaded = 0u64;
        let mut last_reported_mib = 0u64;
        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    file.write_all(&buffer[..n])?;
                    downloaded += n as u64;

                    if let Some(mib) = next_progress_mib(downloaded, last_reported_mib) {
                        last_reported_mib = mib;
                        if let Some(total) = total_size {
                            on_event(DownloadEvent::Progress { downloaded, total });
                        }
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&temp_path);
                    return Err(e.into());
                }
            }
        }

        on_event(DownloadEvent::Finished { downloaded });

        fs::rename(&temp_path, &file_path)?;

        Ok(file_path)
    }

    pub fn list_downloaded(&self) -> Result<Vec<PathBuf>> {
        let mut episodes = Vec::new();

        if !self.download_dir.exists() {
            return Ok(episodes);
        }

        for entry in fs::read_dir(&self.download_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some() {
                let ext = path.extension().unwrap().to_string_lossy();
                if ext == "mp3" || ext == "m4a" || ext == "flac" {
                    episodes.push(path);
                }
            }
        }

        episodes.sort();
        Ok(episodes)
    }

    pub fn is_downloaded(&self, title: &str) -> bool {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);
        file_path.exists()
    }

    pub fn get_path(&self, title: &str) -> Option<PathBuf> {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);

        if file_path.exists() {
            Some(file_path)
        } else {
            None
        }
    }

    /// Deletes a downloaded episode and reports what happened.
    pub fn delete_episode(&self, title: &str) -> Result<DeleteOutcome> {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);

        if file_path.exists() {
            fs::remove_file(&file_path)?;
            Ok(DeleteOutcome::Deleted { filename })
        } else {
            Ok(DeleteOutcome::NotDownloaded)
        }
    }

    pub fn get_total_size(&self) -> Result<u64> {
        let mut total = 0u64;

        if !self.download_dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(&self.download_dir)? {
            let entry = entry?;
            if let Ok(metadata) = entry.metadata() {
                total += metadata.len();
            }
        }

        Ok(total)
    }

    fn sanitize_filename(&self, title: &str) -> String {
        let ext = ".mp3";

        let mut filename = title
            .replace('/', "-")
            .replace('\\', "-")
            .replace(':', "-")
            .replace('*', "")
            .replace('?', "")
            .replace('"', "")
            .replace('<', "")
            .replace('>', "")
            .replace('|', "");

        if filename.len() > 200 {
            filename.truncate(200);
        }

        format!("{}{}", filename, ext)
    }

    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downloader_in(dir: &Path) -> Downloader {
        fs::create_dir_all(dir).unwrap();
        Downloader {
            download_dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn progress_fires_once_per_new_mib_boundary() {
        assert_eq!(next_progress_mib(0, 0), None);
        assert_eq!(next_progress_mib(MIB - 1, 0), None);
        assert_eq!(next_progress_mib(MIB, 0), Some(1));
        // Same MiB again: nothing new.
        assert_eq!(next_progress_mib(MIB + 32 * 1024, 1), None);
        // Crossing with an unaligned total still reports.
        assert_eq!(next_progress_mib(2 * MIB + 5, 1), Some(2));
        // A large jump reports the latest MiB once.
        assert_eq!(next_progress_mib(5 * MIB + 1, 1), Some(5));
    }

    #[test]
    fn already_downloaded_emits_single_event() {
        let dir = std::env::temp_dir().join(format!("mfp-dl-test-a-{}", std::process::id()));
        let d = downloader_in(&dir);
        fs::write(dir.join("Ep 1.mp3"), b"x").unwrap();

        let mut events = Vec::new();
        let path = d
            .download_episode("Ep 1", "http://unused.invalid", |e| events.push(e))
            .unwrap();

        assert_eq!(path, dir.join("Ep 1.mp3"));
        assert_eq!(
            events,
            vec![DownloadEvent::AlreadyDownloaded {
                filename: "Ep 1.mp3".into()
            }]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_reports_outcome() {
        let dir = std::env::temp_dir().join(format!("mfp-dl-test-d-{}", std::process::id()));
        let d = downloader_in(&dir);
        fs::write(dir.join("Ep 2.mp3"), b"x").unwrap();

        assert_eq!(
            d.delete_episode("Ep 2").unwrap(),
            DeleteOutcome::Deleted {
                filename: "Ep 2.mp3".into()
            }
        );
        assert_eq!(
            d.delete_episode("Ep 2").unwrap(),
            DeleteOutcome::NotDownloaded
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
