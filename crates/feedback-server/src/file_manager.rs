use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};
use uuid::Uuid;

const ONE_MB: u64 = 1024 * 1024;
const TEMP_LIFETIME_SECS: u64 = 60 * 60; // 1 hours

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FileStatus {
    Active,
    Expired { deleted_at: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub id: Uuid,
    pub mod_key: String,
    pub original_name: String,
    pub size_bytes: u64,
    pub path: PathBuf,
    pub uploaded_at: u64,
    pub expires_at: Option<u64>,
    pub is_priority: bool,
    pub status: FileStatus,
}

pub struct FileManager {
    pub files: Arc<DashMap<Uuid, FileMetadata>>,
    pub max_storage_bytes: u64,
    pub base_dir: PathBuf,
    pub temp_file_names: DashMap<Uuid, String>,
}

impl FileManager {
    pub fn new(max_storage_mb: u64, base_dir: impl AsRef<Path>) -> Self {
        let max_storage_bytes = max_storage_mb * ONE_MB;
        let base_dir = base_dir.as_ref().to_path_buf();

        // Ensure required directories exist
        let _ = fs::create_dir_all(base_dir.join("TEMP_UPLOADS"));
        let _ = fs::create_dir_all(base_dir.join("ANSWERS"));
        let _ = fs::create_dir_all(base_dir.join("EXPORTS"));

        let files = Arc::new(DashMap::new());

        // Load existing metadata if available
        let index_path = base_dir.join("file_index.json");
        if let Ok(data) = fs::read_to_string(&index_path) {
            if let Ok(parsed_map) = serde_json::from_str::<HashMap<Uuid, FileMetadata>>(&data) {
                for (k, v) in parsed_map {
                    files.insert(k, v);
                }
                info!("Loaded {} files from file_index.json", files.len());
            }
        }

        Self {
            files,
            max_storage_bytes,
            base_dir,
            temp_file_names: DashMap::new(),
        }
    }

    pub fn save_to_disk(&self) {
        let index_path = self.base_dir.join("file_index.json");
        let map: HashMap<_, _> = self.files.iter().map(|kv| (*kv.key(), kv.value().clone())).collect();

        if let Ok(json) = serde_json::to_string_pretty(&map) {
            if let Err(e) = fs::write(&index_path, json) {
                error!("Failed to save file_index.json: {}", e);
            }
        }
    }

    fn current_timestamp() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    // Dynamic lifetime calculation based on file size
    fn calculate_expiration(size_bytes: u64) -> Option<u64> {
        let now = Self::current_timestamp();
        if size_bytes < ONE_MB {
            None // < 1MB: Keep forever (until global limit hit)
        } else if size_bytes <= 10 * ONE_MB {
            Some(now + 30 * 24 * 60 * 60) // 1-10MB: 30 days
        } else {
            Some(now + 7 * 24 * 60 * 60) // > 10MB: 7 days
        }
    }

    pub fn commit_file(
        &self,
        file_id: Uuid,
        mod_key: &str,
        original_name: &str,
        is_priority: bool,
        path: PathBuf,
        size_bytes: u64,
    ) -> Result<(), String> {
        let file_meta = FileMetadata {
            id: file_id,
            mod_key: mod_key.to_string(),
            original_name: original_name.to_string(),
            size_bytes,
            path,
            uploaded_at: Self::current_timestamp(),
            expires_at: Self::calculate_expiration(size_bytes),
            is_priority,
            status: FileStatus::Active,
        };

        self.files.insert(file_id, file_meta);
        self.save_to_disk();

        Ok(())
    }

    /// Registers a file directly into the FMS, moves it from TEMP to ANSWERS
    pub fn commit_temp_file(
        &self,
        temp_file_id: Uuid,
        mod_key: &str,
        user_xuid: &str,
        original_name: &str,
        is_priority: bool,
    ) -> Result<FileMetadata, String> {
        let temp_path = self.base_dir.join("TEMP_UPLOADS").join(temp_file_id.to_string());

        if !temp_path.exists() {
            return Err("Temp file not found or already processed".to_string());
        }

        // TODO: check file size!

        let metadata = fs::metadata(&temp_path).map_err(|e| e.to_string())?;
        let size_bytes = metadata.len();

        let final_dir = self.base_dir.join("ANSWERS").join(mod_key).join(user_xuid).join("files");
        fs::create_dir_all(&final_dir).map_err(|e| e.to_string())?;

        let final_path = final_dir.join(original_name);

        fs::rename(&temp_path, &final_path).map_err(|e| e.to_string())?;

        let file_meta = FileMetadata {
            id: temp_file_id,
            mod_key: mod_key.to_string(),
            original_name: original_name.to_string(),
            size_bytes,
            path: final_path,
            uploaded_at: Self::current_timestamp(),
            expires_at: Self::calculate_expiration(size_bytes),
            is_priority,
            status: FileStatus::Active,
        };

        self.files.insert(temp_file_id, file_meta.clone());
        self.save_to_disk();

        Ok(file_meta)
    }

    /// The core background task loop (should be spawned via tokio)
    pub async fn run_background_tasks(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(3 * 60 * 60)); // Every 3 hours
        loop {
            interval.tick().await;
            info!("Running FMS background tasks...");
            self.cleanup_orphans();
            self.expire_old_files();
            self.enforce_storage_limit();
            self.save_to_disk();
        }
    }

    /// Deletes abandoned files in TEMP_UPLOADS
    fn cleanup_orphans(&self) {
        let temp_dir = self.base_dir.join("TEMP_UPLOADS");
        let now = SystemTime::now();

        if let Ok(entries) = fs::read_dir(temp_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(age) = now.duration_since(modified) {
                            if age.as_secs() > TEMP_LIFETIME_SECS {
                                let _ = fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }

    /// Checks individual file expiration dates
    fn expire_old_files(&self) {
        let now = Self::current_timestamp();
        let mut to_expire = Vec::new();

        for entry in self.files.iter() {
            let meta = entry.value();
            if let FileStatus::Active = meta.status {
                if let Some(exp) = meta.expires_at {
                    if now >= exp {
                        to_expire.push(*entry.key());
                    }
                }
            }
        }

        for id in to_expire {
            self.mark_as_expired(&id);
        }
    }

    fn mark_as_expired(&self, id: &Uuid) {
        if let Some(mut meta) = self.files.get_mut(id) {
            let _ = fs::remove_file(&meta.path); // Delete physically
            meta.status = FileStatus::Expired { deleted_at: Self::current_timestamp() };
        }
    }

    /// Smart Deletion Algorithm: Targets the heaviest mod_key first
    fn enforce_storage_limit(&self) {
        let mut total_active_bytes: u64 = 0;
        let mut usage_per_key: HashMap<String, u64> = HashMap::new();

        // 1. Calculate current usage
        for entry in self.files.iter() {
            let meta = entry.value();
            if let FileStatus::Active = meta.status {
                total_active_bytes += meta.size_bytes;
                *usage_per_key.entry(meta.mod_key.clone()).or_insert(0) += meta.size_bytes;
            }
        }

        if total_active_bytes <= self.max_storage_bytes {
            return; // Under limit, all good
        }

        warn!("Storage limit exceeded ({} / {} bytes). Running Smart Deletion...", total_active_bytes, self.max_storage_bytes);
        let target_bytes = (self.max_storage_bytes as f64 * 0.9) as u64; // Target 90% capacity

        // 2. Delete files until we reach target capacity
        while total_active_bytes > target_bytes {
            // Find the biggest offender (mod_key with highest usage)
            let biggest_offender = usage_per_key.iter()
                .max_by_key(|(_, usage)| **usage)
                .map(|(key, _)| key.clone());

            let target_mod_key = match biggest_offender {
                Some(key) => key,
                None => break, // No more files to process
            };

            // Find their oldest, non-priority active file
            let mut oldest_file_id = None;
            let mut oldest_time = u64::MAX;

            for entry in self.files.iter() {
                let meta = entry.value();
                if let FileStatus::Active = meta.status {
                    if meta.mod_key == target_mod_key && !meta.is_priority {
                        if meta.uploaded_at < oldest_time {
                            oldest_time = meta.uploaded_at;
                            oldest_file_id = Some(*entry.key());
                        }
                    }
                }
            }

            if let Some(id) = oldest_file_id {
                // We found a file to delete
                let size_freed = self.files.get(&id).unwrap().size_bytes;
                self.mark_as_expired(&id);

                total_active_bytes -= size_freed;
                if let Some(usage) = usage_per_key.get_mut(&target_mod_key) {
                    *usage -= size_freed;
                }
                info!("Smart Deletion: Removed file {} from {} (freed {} bytes)", id, target_mod_key, size_freed);
            } else {
                // The biggest offender only has priority files. Remove them from our target list.
                usage_per_key.remove(&target_mod_key);
                if usage_per_key.is_empty() {
                    error!("CRITICAL: Cannot free enough space! All remaining files are priority.");
                    break;
                }
            }
        }
    }
}
