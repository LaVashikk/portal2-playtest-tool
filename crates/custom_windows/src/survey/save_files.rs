use std::{fs::File, io::{self, Read, Write}, path::{Path, PathBuf}, sync::{LazyLock, Mutex, atomic::AtomicU16}};

use anyhow::Result;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{ENGINE, survey::{ClientConfig, SURVEY_ANSWERS_RELATIVE}};

pub static DEMO_FILES: LazyLock<Mutex<Vec<PathBuf>>> = LazyLock::new(|| Mutex::new(Vec::new()));
pub static LOGS_FILE: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));
pub static VIDEO_FILE: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

pub static LAST_MAP_NAME: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
pub static LAST_DEMO_INDEX: AtomicU16 = AtomicU16::new(0);

pub fn init_saver(config: &ClientConfig) {
    let engine = ENGINE.get().unwrap();
    let save_demo = config.save_demos;
    let save_logs = config.save_console_logs;
    let save_rec = config.save_recordings;

    if !save_demo && !save_logs && !save_rec {
        return;
    }

    engine.game_event_manager().listen("player_connect", move |_| {
        let engine = ENGINE.get().unwrap();
        if save_demo {
            let map_name = LAST_MAP_NAME.lock().unwrap().clone();
            let demo_index = LAST_DEMO_INDEX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let demo_name = format!("demo_{}_{}_{}.dem", map_name, demo_index, super::get_timestamp());
            let command = format!("record {}/demos/{}", SURVEY_ANSWERS_RELATIVE, demo_name);
            let full_path = super::get_answer_dir().join("demos").join(demo_name);
            DEMO_FILES.lock().unwrap().push(full_path);

            engine.client().execute_client_cmd_unrestricted("stop");
            engine.client().execute_client_cmd_unrestricted(&command);
        }
    });

    engine.game_event_manager().listen("server_spawn", move |event| {
        let engine = ENGINE.get().unwrap();
        let timestamp = super::get_timestamp();

        let map_name = event.get_string("mapname", "unknown");
        if let Ok(mut last_map) = LAST_MAP_NAME.lock() {
            if *last_map == map_name {
                return;
            }

            *last_map = map_name.clone();
            DEMO_FILES.lock().unwrap().clear();
            LOGS_FILE.lock().unwrap().take();
            VIDEO_FILE.lock().unwrap().take();
        }

        if save_demo {
            // processing in "player_connect" event. Because here too early for engine
        }

        if save_logs {
            let log_name = format!("{}_console_{}.log", map_name, timestamp);
            let full_path = super::get_answer_dir().join("logs").join(&log_name);
            LOGS_FILE.lock().unwrap().replace(full_path);

            let command = format!("con_logfile {}/logs/{}", SURVEY_ANSWERS_RELATIVE, log_name);
            engine.client().execute_client_cmd_unrestricted(&command);
        }

        if save_rec {
            start_recording(&map_name);
        }

    });
}

pub fn stop_demo_recording() {
    let engine = ENGINE.get().unwrap();
    engine.client().execute_client_cmd_unrestricted("stop");
}

pub fn pack_demos() -> Result<PathBuf> {
    let map_name = LAST_MAP_NAME.lock().unwrap().clone();
    let zip_path = super::get_answer_dir().join("demos").join(format!("demos_{}_{}.zip", map_name, super::get_timestamp()));

    let file = File::create(&zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    for file_path in DEMO_FILES.lock().unwrap().iter() {
        let file = File::open(file_path)?;
        let file_name = file_path.file_name().unwrap().to_str().unwrap();
        zip.start_file(file_name, options)?;

        let mut buffer = Vec::new();
        io::copy(&mut file.take(u64::MAX), &mut buffer)?;

        zip.write_all(&buffer)?;
    }

    zip.finish()?;
    Ok(zip_path)
}

pub fn start_recording(map_name: &str) {
    if let Ok(mut recorder) = recorder::RECORDER.get().unwrap().lock() {
        if recorder.is_running() {
            let _ = recorder.stop_recording();
        }

        let timestamp = super::get_timestamp();
        let video_full_path = super::get_answer_dir
            ().join("records").join(format!("recording_{}_{}.mp4", map_name, timestamp));

        let game_resolution = ENGINE.get().expect("unreachable").client().get_screen_size();
        let rec_res = recorder::calc_aligned_resolution(game_resolution.0 as u32, game_resolution.1 as u32);

        let _ = recorder.start_recording(&video_full_path, rec_res);
        VIDEO_FILE.lock().unwrap().replace(video_full_path);
    }
}

pub fn stop_recording() {
    if let Ok(mut recorder) = recorder::RECORDER.get().unwrap().lock() {
        if let Err(e) = recorder.stop_recording() {
            log::warn!("Failed to stop recording: {}", e);
        }
    }
}
