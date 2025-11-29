use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CHUNK_SIZE: usize = 32 * 1024; // Chunk size: 32 KB

pub struct Downloader {
    download_dir: PathBuf,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let download_dir = dirs::config_dir()
            .context("No se pudo obtener el directorio de configuración")?
            .join("mfp")
            .join("downloads");

        fs::create_dir_all(&download_dir)?;

        Ok(Downloader { download_dir })
    }

    pub fn download_episode(&self, title: &str, url: &str) -> Result<PathBuf> {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);

        if file_path.exists() {
            println!("Episode already downloaded: {}", filename);
            return Ok(file_path);
        }

        println!("Downloading: {}", title);

        let mut response = reqwest::blocking::get(url)
            .context("No se pudo conectar al servidor")?;

        if !response.status().is_success() {
            anyhow::bail!("Error HTTP: {}", response.status());
        }

        let total_size = response.content_length();

        let temp_path = file_path.with_extension("tmp");
        let mut file = File::create(&temp_path)
            .context("No se pudo crear el archivo")?;

        let mut downloaded = 0u64;
        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    file.write_all(&buffer[..n])?;
                    downloaded += n as u64;

                    if downloaded % (1024 * 1024) == 0 {
                        if let Some(total) = total_size {
                            let percent = (downloaded as f64 / total as f64) * 100.0;
                            print!("\r  Progress: {:.1}% ({:.1}/{:.1} MB)",
                                percent,
                                downloaded as f64 / 1_048_576.0,
                                total as f64 / 1_048_576.0
                            );
                            std::io::stdout().flush().ok();
                        }
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&temp_path);
                    return Err(e.into());
                }
            }
        }

        println!("\rDownload complete: {:.2} MB                    ",
            downloaded as f64 / 1_048_576.0);

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

    pub fn delete_episode(&self, title: &str) -> Result<()> {
        let filename = self.sanitize_filename(title);
        let file_path = self.download_dir.join(&filename);

        if file_path.exists() {
            fs::remove_file(&file_path)?;
            println!("Deleted: {}", filename);
        } else {
            println!("Episode not downloaded");
        }

        Ok(())
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
