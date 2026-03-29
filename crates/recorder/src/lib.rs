use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use thiserror::Error;

// Recorder specific statics
pub const FFMPEG_PATH: &str = "ffmpeg.exe";
pub static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();
pub const FRAME_SKIP: usize = 10;
pub const RECORDING_FPS: i32 = 20; // todo: place it in config file, not hard-coded
pub const CAPTURE_SCALE: u32 = 2; // Capture at 1/2 resolution (e.g., 1920x1080 -> 960x540)


#[derive(Error, Debug)]
pub enum RecorderError {
    #[error("Recorder is already running")]
    AlreadyRunning,
    #[error("Recorder is not running")]
    NotRunning,
    #[error("FFmpeg executable not found: {0}")]
    FFmpegNotFound(String),
    #[error("FFmpeg command failed to start: {0}")]
    ProcessStart(std::io::Error),
    #[error("Failed to acquire stdin of FFmpeg process")]
    StdinAcquire,
    #[error("Failed to write frame to FFmpeg: {0}")]
    FrameWrite(std::io::Error),
    #[error("Failed to send frame to writer thread")]
    FrameSendError,
    #[error("Writer thread panicked")]
    ThreadPanic,
}

pub struct Recorder {
    is_running: bool,
    resolution: (u32, u32),
    writer_thread: Option<JoinHandle<Result<(), RecorderError>>>,
    frame_sender: Option<Sender<Vec<u8>>>,
    ffmpeg_path: PathBuf,
}

impl Recorder {
    pub fn new(width: u32, height: u32, ffmpeg_path: impl AsRef<Path>) -> Self {
        Self {
            is_running: false,
            resolution: (width, height),
            writer_thread: None,
            frame_sender: None,
            ffmpeg_path: ffmpeg_path.as_ref().to_path_buf(),
        }
    }

    pub fn start_recording(
        &mut self,
        output_path: impl AsRef<Path>,
        fps: i32,
    ) -> Result<(), RecorderError> {
        if self.is_running {
            return Err(RecorderError::AlreadyRunning);
        }

        let (w, h) = self.resolution;
        let (frame_sender, frame_receiver) = channel::<Vec<u8>>();
        self.frame_sender = Some(frame_sender);

        let output_path = output_path.as_ref().to_path_buf();
        let ffmpeg_path = self.ffmpeg_path.clone();

        self.writer_thread = Some(thread::spawn(move || {
            writer_thread_main(output_path, w, h, fps, ffmpeg_path, frame_receiver)
        }));

        self.is_running = true;
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<(), RecorderError> {
        if !self.is_running {
            return Err(RecorderError::NotRunning);
        }

        // Dropping the sender will cause the writer thread to terminate its loop.
        self.frame_sender.take();

        if let Some(handle) = self.writer_thread.take() {
            match handle.join() {
                Ok(Ok(_)) => (),
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(RecorderError::ThreadPanic),
            }
        }

        self.is_running = false;
        Ok(())
    }

    pub fn send_frame(&self, frame_data: Vec<u8>) -> Result<(), RecorderError> {
        if self.frame_sender.is_none() {
            return Err(RecorderError::NotRunning);
        }

        self.frame_sender
            .as_ref()
            .unwrap()
            .send(frame_data)
            .map_err(|_| RecorderError::FrameSendError)
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HwEncoder {
    NvidiaNvenc,
    AmdAmf,
    IntelQsv,
    Software, // Fallback (libx264)
}

pub fn detect_best_hw_encoder() -> HwEncoder {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    let mut best_encoder = HwEncoder::Software;

    unsafe {
        let factory: Result<IDXGIFactory1, _> = CreateDXGIFactory1();
        let factory = match factory {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Failed to create DXGI Factory: {}. Falling back to software encoder.", e);
                return best_encoder;
            }
        };

        let mut adapter_index = 0;

        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            if let Ok(desc) = adapter.GetDesc1() {
                if (desc.Flags & 2) != 0 { // DXGI_ADAPTER_FLAG_SOFTWARE
                    adapter_index += 1;
                    continue;
                }

                // 0x10DE - NVIDIA
                // 0x1002 - AMD
                // 0x8086 - Intel
                match desc.VendorId {
                    0x10DE => {
                        log::info!("Detected NVIDIA GPU. Enabling NVENC.");
                        return HwEncoder::NvidiaNvenc;
                    }
                    0x1002 => {
                        log::info!("Detected AMD GPU. Enabling AMF.");
                        return HwEncoder::AmdAmf;
                    }
                    0x8086 => {
                        log::info!("Detected Intel GPU.");
                        if best_encoder == HwEncoder::Software {
                            best_encoder = HwEncoder::IntelQsv;
                        }
                    }
                    _ => {
                        log::debug!("Detected unknown GPU Vendor ID: {}", desc.VendorId);
                    }
                }
            }
            adapter_index += 1;
        }
    }

    best_encoder
}

fn writer_thread_main(
    output_path: impl AsRef<Path>,
    width: u32,
    height: u32,
    fps: i32,
    ffmpeg_path: impl AsRef<Path>,
    frame_receiver: Receiver<Vec<u8>>,
) -> Result<(), RecorderError> {
    if !ffmpeg_path.as_ref().is_file() {
        let err_msg = format!(
            "ffmpeg.exe not found at path: '{}'. Please place it in the same directory as the DLL.",
            ffmpeg_path.as_ref().display()
        );
        return Err(RecorderError::FFmpegNotFound(err_msg));
    }

    let encoder = detect_best_hw_encoder();
    let size_str = format!("{}x{}", width, height);
    let fps_str = fps.to_string();

    let mut args = vec![
        "-f", "rawvideo",
        "-pix_fmt", "bgra",
        "-s", &size_str,
        "-r", &fps_str,
        "-i", "-", // Read from stdin
        // "-c:v", "libx264",
        // "-preset", "superfast",
        // "-crf", "24", // Constant Rate Factor, good balance of quality and size
        "-y", // Overwrite output file if it exists
        output_path.as_ref().to_str().unwrap(),
    ];

    match encoder {
        HwEncoder::NvidiaNvenc => {
            args.extend_from_slice(&[
                "-c:v", "h264_nvenc",
                "-preset", "p3",
                "-cq", "24",
            ]);
        }
        HwEncoder::AmdAmf => {
            args.extend_from_slice(&[
                "-c:v", "h264_amf",
                "-quality", "speed",
                "-qp_i", "24", "-qp_p", "24", "-qp_b", "24",
            ]);
        }
        HwEncoder::IntelQsv => {
            args.extend_from_slice(&[
                "-c:v", "h264_qsv",
                "-preset", "fast",
                "-global_quality", "24",
            ]);
        }
        HwEncoder::Software => {
            args.extend_from_slice(&[
                "-c:v", "libx264",
                "-preset", "superfast",
                "-crf", "24",
            ]);
        }
    }

    dbg!(&args);

    let mut child = Command::new(ffmpeg_path.as_ref())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null()) // Or Stdio::piped() to log ffmpeg output
        .stderr(Stdio::piped()) // Capture stderr to log errors
        .spawn()
        .map_err(RecorderError::ProcessStart)?;

    let mut stdin = child.stdin.take().ok_or(RecorderError::StdinAcquire)?;

    // This thread will log FFmpeg's stderr output without blocking
    let stderr = child.stderr.take();
    let stderr_thread = thread::spawn(move || {
        if let Some(stderr) = stderr {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => log::debug!("[FFmpeg] {}", line),
                    Err(e) => log::error!("[FFmpeg] Error reading stderr: {}", e),
                }
            }
        }
    });

    while let Ok(frame) = frame_receiver.recv() {
        if let Err(e) = stdin.write_all(&frame) { // it's fkung bottleneck, and it's peace of shit!!!!!!!!
            // Stop waiting for frames if pipe is broken
            log::error!("Failed to write to FFmpeg stdin: {}. Stopping.", e);
            break;
        }
    }

    // stdin is closed when it goes out of scope here, signaling EOF to ffmpeg
    drop(stdin);

    let status = child.wait().map_err(RecorderError::ProcessStart)?;
    log::debug!("FFmpeg process exited with status: {}", status);

    stderr_thread.join().unwrap();

    Ok(())
}
