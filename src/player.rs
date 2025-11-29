use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BUFFER_SIZE: usize = 512 * 1024; // Initial buffer: 512 KB
const CHUNK_SIZE: usize = 32 * 1024;   // Chunk size: 32 KB

struct StreamingBuffer {
    buffer: Arc<Mutex<Vec<u8>>>,
    position: usize,
    download_complete: Arc<Mutex<bool>>,
}

impl StreamingBuffer {
    fn new(buffer: Arc<Mutex<Vec<u8>>>, download_complete: Arc<Mutex<bool>>) -> Self {
        Self {
            buffer,
            position: 0,
            download_complete,
        }
    }
}

impl Read for StreamingBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let buffer = self.buffer.lock().unwrap();
            let available = buffer.len();

            if self.position < available {
                let remaining = available - self.position;
                let to_read = remaining.min(buf.len());

                buf[..to_read].copy_from_slice(&buffer[self.position..self.position + to_read]);
                self.position += to_read;

                return Ok(to_read);
            }

            let is_complete = *self.download_complete.lock().unwrap();
            if is_complete {
                return Ok(0);
            }

            drop(buffer);
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Seek for StreamingBuffer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let buffer = self.buffer.lock().unwrap();
        let buffer_len = buffer.len() as i64;

        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::Current(offset) => self.position as i64 + offset,
            SeekFrom::End(offset) => buffer_len + offset,
        };

        if new_pos < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot seek before beginning",
            ));
        }

        self.position = new_pos as usize;
        Ok(self.position as u64)
    }
}

pub struct Player {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Arc<Mutex<Option<Arc<Sink>>>>,
    playback_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    download_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    is_paused: Arc<Mutex<bool>>,
    start_time: Arc<Mutex<Option<Instant>>>,
    paused_duration: Arc<Mutex<Duration>>,
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()
            .context("No se pudo inicializar el dispositivo de audio. Verifica tu configuración de audio.")?;

        Ok(Player {
            _stream: stream,
            stream_handle,
            sink: Arc::new(Mutex::new(None)),
            playback_thread: Arc::new(Mutex::new(None)),
            download_thread: Arc::new(Mutex::new(None)),
            is_paused: Arc::new(Mutex::new(false)),
            start_time: Arc::new(Mutex::new(None)),
            paused_duration: Arc::new(Mutex::new(Duration::from_secs(0))),
        })
    }

    pub fn play(&self, url: &str) -> Result<()> {
        self.stop();

        *self.start_time.lock().unwrap() = Some(Instant::now());
        *self.paused_duration.lock().unwrap() = Duration::from_secs(0);

        print!("Connecting...");
        use std::io::Write;
        std::io::stdout().flush().ok();

        let sink = Arc::new(Sink::try_new(&self.stream_handle)
            .context("No se pudo crear el sink de audio")?);

        *self.sink.lock().unwrap() = Some(Arc::clone(&sink));
        *self.is_paused.lock().unwrap() = false;

        let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();
        let download_complete = Arc::new(Mutex::new(false));
        let download_complete_clone = Arc::clone(&download_complete);

        let url = url.to_string();
        let download_handle = thread::spawn(move || {
            let _ = Self::download_stream(&url, tx, download_complete_clone);
        });

        let sink_clone = Arc::clone(&sink);
        let playback_handle = thread::spawn(move || {
            let _ = Self::play_stream(rx, &sink_clone, download_complete);
        });

        *self.download_thread.lock().unwrap() = Some(download_handle);
        *self.playback_thread.lock().unwrap() = Some(playback_handle);

        std::thread::sleep(std::time::Duration::from_millis(1500));

        Ok(())
    }

    fn download_stream(url: &str, tx: Sender<Vec<u8>>, download_complete: Arc<Mutex<bool>>) -> Result<()> {
        let mut response = reqwest::blocking::get(url)
            .context("No se pudo conectar al servidor")?;

        if !response.status().is_success() {
            anyhow::bail!("Error HTTP: {}", response.status());
        }

        let mut buffer = vec![0u8; CHUNK_SIZE];

        loop {
            match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }

        *download_complete.lock().unwrap() = true;

        Ok(())
    }

    fn play_stream(rx: Receiver<Vec<u8>>, sink: &Sink, download_complete: Arc<Mutex<bool>>) -> Result<()> {
        let mut initial_buffer = Vec::new();

        print!(" buffering...");
        use std::io::Write;
        std::io::stdout().flush().ok();

        while initial_buffer.len() < BUFFER_SIZE {
            match rx.recv() {
                Ok(chunk) => initial_buffer.extend_from_slice(&chunk),
                Err(_) => {
                    if initial_buffer.is_empty() {
                        anyhow::bail!("No se recibieron datos");
                    }
                    break;
                }
            }
        }

        println!(" OK\n");

        let buffer_arc = Arc::new(Mutex::new(initial_buffer));
        let buffer_clone = Arc::clone(&buffer_arc);

        thread::spawn(move || {
            while let Ok(chunk) = rx.recv() {
                buffer_clone.lock().unwrap().extend_from_slice(&chunk);
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(200));

        let streaming_buffer = StreamingBuffer::new(buffer_arc, download_complete);
        let buf_reader = BufReader::new(streaming_buffer);

        let source = Decoder::new(buf_reader)
            .context("No se pudo decodificar el audio")?;

        sink.append(source);
        sink.sleep_until_end();

        Ok(())
    }

    pub fn stop(&self) {
        if let Some(sink) = self.sink.lock().unwrap().take() {
            sink.stop();
        }

        let _ = self.playback_thread.lock().unwrap().take();
        let _ = self.download_thread.lock().unwrap().take();

        *self.is_paused.lock().unwrap() = false;
    }

    pub fn pause(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            if !*self.is_paused.lock().unwrap() {
                sink.pause();
                *self.is_paused.lock().unwrap() = true;
                if let Some(start) = *self.start_time.lock().unwrap() {
                    let elapsed = start.elapsed();
                    *self.paused_duration.lock().unwrap() = elapsed;
                }
            }
        }
    }

    pub fn resume(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            if *self.is_paused.lock().unwrap() {
                sink.play();
                *self.is_paused.lock().unwrap() = false;
                let paused = *self.paused_duration.lock().unwrap();
                *self.start_time.lock().unwrap() = Some(Instant::now() - paused);
            }
        }
    }

    pub fn is_paused(&self) -> bool {
        *self.is_paused.lock().unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.sink
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.empty())
            .unwrap_or(true)
    }

    pub fn volume(&self) -> f32 {
        self.sink
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.volume())
            .unwrap_or(1.0)
    }

    pub fn set_volume(&self, volume: f32) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.set_volume(volume.clamp(0.0, 2.0));
        }
    }

    pub fn sleep_until_end(&self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.sleep_until_end();
        }
    }

    pub fn elapsed_seconds(&self) -> u64 {
        if let Some(start) = *self.start_time.lock().unwrap() {
            if *self.is_paused.lock().unwrap() {
                self.paused_duration.lock().unwrap().as_secs()
            } else {
                start.elapsed().as_secs()
            }
        } else {
            0
        }
    }
}

pub fn parse_duration(duration_str: &str) -> Option<u64> {
    let parts: Vec<&str> = duration_str.split(':').collect();

    match parts.len() {
        2 => {
            let minutes = parts[0].parse::<u64>().ok()?;
            let seconds = parts[1].parse::<u64>().ok()?;
            Some(minutes * 60 + seconds)
        }
        3 => {
            let hours = parts[0].parse::<u64>().ok()?;
            let minutes = parts[1].parse::<u64>().ok()?;
            let seconds = parts[2].parse::<u64>().ok()?;
            Some(hours * 3600 + minutes * 60 + seconds)
        }
        _ => None,
    }
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
