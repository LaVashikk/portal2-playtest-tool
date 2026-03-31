use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::os::windows::process::CommandExt;
use std::sync::{Mutex, OnceLock};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use thiserror::Error;

// Recorder specific statics
pub const FFMPEG_PATH: &str = "ffmpeg.exe";
pub static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();
pub static FRAME_SKIP: OnceLock<usize> = OnceLock::new();
pub static RECORDING_FPS: OnceLock<i32> = OnceLock::new();
pub static CAPTURE_RESOLUTION: OnceLock<u32> = OnceLock::new();

pub fn init_recorder_const(frame_skip: usize, recording_fps: i32, recording_resolution: u32) {
    FRAME_SKIP.set(frame_skip).unwrap();
    RECORDING_FPS.set(recording_fps).unwrap();
    CAPTURE_RESOLUTION.set(recording_resolution).unwrap();
}

pub enum RecorderCommand {
    Frame(Vec<u8>),
    Flush,
}

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
    resolution: (u32, u32), // TODO! REMOVE IT!!!!!
    writer_thread: Option<JoinHandle<Result<(), RecorderError>>>,
    frame_sender: Option<Sender<RecorderCommand>>,
    ffmpeg_path: PathBuf,
    pub last_record_path: Option<PathBuf>,
}

impl Recorder {
    pub fn new(width: u32, height: u32, ffmpeg_path: impl AsRef<Path>) -> Self {
        Self {
            is_running: false,
            resolution: (width, height),
            writer_thread: None,
            frame_sender: None,
            ffmpeg_path: ffmpeg_path.as_ref().to_path_buf(),
            last_record_path: None,
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
        let (frame_sender, frame_receiver) = channel();
        self.frame_sender = Some(frame_sender);

        let output_path = output_path.as_ref().to_path_buf();
        let ffmpeg_path = self.ffmpeg_path.clone();

        self.last_record_path = Some(output_path.clone());
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
        // log::debug!("Send frame: {}", frame_data.len());
        if let Some(sender) = &self.frame_sender {
            sender.send(RecorderCommand::Frame(frame_data)).map_err(|_| RecorderError::FrameSendError)
        } else {
            Err(RecorderError::NotRunning)
        }
    }

    pub fn flush(&self) -> Result<(), RecorderError> {
        if let Some(sender) = &self.frame_sender {
            sender.send(RecorderCommand::Flush).map_err(|_| RecorderError::StdinAcquire)
        } else {
            Err(RecorderError::NotRunning)
        }
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

// Checking if hardware encoder DLLs are available
fn is_hw_encoder_dll_available(dll_name: &str) -> bool {
    use windows::Win32::System::LibraryLoader::LoadLibraryA;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::core::PCSTR;

    let mut name = String::from(dll_name);
    name.push('\0');

    unsafe {
        match LoadLibraryA(PCSTR(name.as_ptr())) {
            Ok(h_module) if !h_module.is_invalid() => {
                let _ = FreeLibrary(h_module);
                true
            }
            _ => false,
        }
    }
}


pub fn detect_best_hw_encoder() -> HwEncoder {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    static BEST_ENCODER: OnceLock<HwEncoder> = OnceLock::new();
    if let Some(encoder) = BEST_ENCODER.get() {
        return *encoder;
    }

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

                match desc.VendorId {
                    0x10DE => { // Nvidia
                        if is_hw_encoder_dll_available("nvEncodeAPI.dll") {
                            log::info!("Detected NVIDIA GPU. Enabling NVENC.");
                            return HwEncoder::NvidiaNvenc;
                        } else {
                            log::warn!("NVIDIA GPU detected, but nvEncodeAPI.dll is missing. Fallback to software.");
                        }
                    }
                    0x1002 => { // AMD
                        if is_hw_encoder_dll_available("amfrt32.dll") {
                            log::info!("Detected AMD GPU. Enabling AMF.");
                            return HwEncoder::AmdAmf;
                        } else {
                            log::warn!("AMD GPU detected, but amfrt32.dll is missing. Fallback to software.");
                        }
                    }
                    0x8086 => {
                        if is_hw_encoder_dll_available("libmfxhw32.dll") {
                            log::info!("Detected Intel GPU. Enabling QSV.");
                            if best_encoder == HwEncoder::Software {
                                best_encoder = HwEncoder::IntelQsv;
                            }
                        } else {
                            log::warn!("Intel GPU detected, but libmfxhw32.dll is missing. Fallback to software.");
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

    let _ = BEST_ENCODER.set(best_encoder);
    best_encoder
}

fn writer_thread_main(
    output_path: impl AsRef<Path>,
    width: u32,
    height: u32,
    fps: i32,
    ffmpeg_path: impl AsRef<Path>,
    frame_receiver: Receiver<RecorderCommand>,
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
    ];

    match encoder {
        HwEncoder::NvidiaNvenc => {
            args.extend_from_slice(&[
                "-c:v", "h264_nvenc",
                "-preset", "p3",
                "-cq", "24",
                "-pix_fmt", "yuv420p",
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

    args.extend_from_slice(&[
        "-y",
        output_path.as_ref().to_str().unwrap(),
    ]);

    dbg!(&args);

    let mut child = Command::new(ffmpeg_path.as_ref())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0)
        .spawn()
        .map_err(RecorderError::ProcessStart)?;

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

    let stdin = child.stdin.take().ok_or(RecorderError::StdinAcquire)?;
    let mut buffered_stdin = BufWriter::with_capacity(1024 * 1024 * 3, stdin);
    // let mut buffered_stdin = stdin;

    while let Ok(cmd) = frame_receiver.recv() {
        match cmd {
            RecorderCommand::Frame(frame) => {
                if let Err(e) = buffered_stdin.write_all(&frame) { // it's fkung bottleneck, and it's peace of shit!! half-fixed with BufWriter
                    // Stop waiting for frames if pipe is broken
                    log::error!("Failed to write to FFmpeg stdin: {}. Stopping.", e);
                    break;
                }
            },
            RecorderCommand::Flush => {
                if let Err(e) = buffered_stdin.flush() {
                    log::error!("Failed to flush to FFmpeg: {}", e);
                }
            },
        }
    }

    // stdin is closed when it goes out of scope here, signaling EOF to ffmpeg
    let _ = buffered_stdin.flush();
    drop(buffered_stdin);

    // who dare?
    log::warn!("ffmpeg ded.");
    let status = child.wait().map_err(RecorderError::ProcessStart)?;
    log::debug!("FFmpeg process exited with status: {}", status);

    stderr_thread.join().unwrap();

    Ok(())
}
