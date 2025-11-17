use crate::models::{ModeratorKeyData, SubmissionEvent};
use dashmap::DashMap;
use indexmap::IndexMap;
use serde::{de::DeserializeOwned, Serialize};
use serenity::prelude::TypeMapKey;
use tracing::{info, warn};
use uuid::Uuid;
use std::{fs, io, path::PathBuf, sync::Arc};
use tokio::sync::broadcast;

// alias for thread-safe key store
pub type KeyStore = Arc<DashMap<String, ModeratorKeyData>>;
// alias for safety access to files
pub type DataIndex = Arc<DashMap<Uuid, PathBuf>>;

// The shared application state
#[derive(Clone)]
pub struct ServerState {
    pub key_store: KeyStore,
    pub submission_sender: broadcast::Sender<SubmissionEvent>,
    pub data_index: DataIndex,
}

impl TypeMapKey for ServerState {
    type Value = Self;
}

impl ServerState {
    pub fn new() -> Self {
        let key_store = load_map_from_disk::<String, ModeratorKeyData>("keys.json").unwrap_or_else(|e| {
            warn!("Could not load keys.json: {}. Starting empty.", e);
            Arc::new(DashMap::new())
        });
        info!("Loaded {} keys from disk.", key_store.len());

        let data_index = load_map_from_disk::<Uuid, PathBuf>("index.json").unwrap_or_else(|e| {
            warn!("Could not load index.json: {}. Starting empty.", e);
            Arc::new(DashMap::new())
        });
        info!("Loaded {} index entries from disk.", data_index.len());

        let (sender, _) = broadcast::channel(100);

        Self {
            key_store,
            submission_sender: sender,
            data_index,
        }
    }

    pub fn save_state_to_disk(&self) -> io::Result<()> { // sqlite? no, thanks! :PP
        save_map_to_disk("keys.json", &self.key_store)?;
        save_map_to_disk("index.json", &self.data_index)?;
        Ok(())
    }

}

// Generic function to load any DashMap from a JSON file
fn load_map_from_disk<K, V>(path: &str) -> Result<Arc<DashMap<K, V>>, io::Error>
where
    K: Eq + std::hash::Hash + Ord + DeserializeOwned + Clone,
    V: DeserializeOwned + Clone,
{
    let data = fs::read_to_string(path)?;
    let hash_map: IndexMap<K, V> = serde_json::from_str(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let dashmap = DashMap::new();
    for (k, v) in hash_map {
        dashmap.insert(k, v);
    }
    Ok(Arc::new(dashmap))
}

fn save_map_to_disk<K, V>(path: &str, map: &DashMap<K, V>) -> io::Result<()>
where
    K: Eq + std::hash::Hash + Ord + Clone + Serialize,
    V: Clone + Serialize,
{
    let hash_map: IndexMap<K, V> = map.iter().map(|item| (item.key().clone(), item.value().clone())).collect();
    let json_data = serde_json::to_string_pretty(&hash_map)?;
    fs::write(path, json_data)
}
