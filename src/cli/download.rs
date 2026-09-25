use crate::operations::downloads::{DeleteOutcome, DownloadEvent, Downloader};
use std::io::Write;
use crate::operations::episodes::find_by_number;
use crate::operations::feed::Feed;
use anyhow::Result;

/// Downloads, lists, deletes or reports disk usage of offline episodes.
pub(super) fn manage_downloads(
    episode: Option<usize>,
    list: bool,
    delete: Option<String>,
    size: bool,
) -> Result<()> {
    let downloader = Downloader::new()?;

    if size {
        let total_bytes = downloader.get_total_size()?;
        let total_mb = total_bytes as f64 / 1_048_576.0;
        println!("Disk usage: {:.2} MB", total_mb);
        println!("Location: {}", downloader.download_dir().display());
        return Ok(());
    }

    if list {
        let downloaded = downloader.list_downloaded()?;
        if downloaded.is_empty() {
            println!("No downloaded episodes");
        } else {
            println!("Downloaded episodes ({}):", downloaded.len());
            for path in downloaded {
                if let Some(filename) = path.file_name() {
                    println!("  - {}", filename.to_string_lossy());
                }
            }
        }
        return Ok(());
    }

    if let Some(title) = delete {
        match downloader.delete_episode(&title)? {
            DeleteOutcome::Deleted { filename } => println!("Deleted: {}", filename),
            DeleteOutcome::NotDownloaded => println!("Episode not downloaded"),
        }
        return Ok(());
    }

    if let Some(ep_num) = episode {
        println!("Obteniendo episodio...");
        let feed = Feed::fetch()?;

        if let Some(ep) = find_by_number(feed.episodes(), ep_num) {
            downloader.download_episode(&ep.title, &ep.audio_url, render_download_event)?;
        } else {
            println!("Episode {} not found", ep_num);
        }
        return Ok(());
    }

    println!("Gestión de descargas offline");
    println!("\nUso:");
    println!("  mfp download -e 75        Descargar episodio 75");
    println!("  mfp download --list       Listar descargados");
    println!("  mfp download --size       Mostrar espacio usado");
    println!("  mfp download --delete \"Episode 75\"  Eliminar episodio");

    Ok(())
}

/// Prints a download event to the terminal exactly as the plain CLI always did.
pub(super) fn render_download_event(event: DownloadEvent) {
    match event {
        DownloadEvent::AlreadyDownloaded { filename } => {
            println!("Episode already downloaded: {}", filename)
        }
        DownloadEvent::Started { title } => println!("Downloading: {}", title),
        DownloadEvent::Progress { downloaded, total } => {
            let percent = (downloaded as f64 / total as f64) * 100.0;
            print!(
                "\r  Progress: {:.1}% ({:.1}/{:.1} MB)",
                percent,
                downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            );
            std::io::stdout().flush().ok();
        }
        DownloadEvent::Finished { downloaded } => println!(
            "\rDownload complete: {:.2} MB                    ",
            downloaded as f64 / 1_048_576.0
        ),
    }
}
