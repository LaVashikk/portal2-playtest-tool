use std::ffi::{CStr, c_char, c_int};

pub const MAX_PLAYER_NAME_LENGTH: usize = 128;
pub const SIGNED_GUID_LEN: usize = 32;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    /// SteamID64
    pub xuid: u64,

    /// Player name
    pub name: [c_char; MAX_PLAYER_NAME_LENGTH],

    /// Unique ID on the server (1, 2, 3, etc.)
    pub user_id: c_int,

    /// SteamID2 as a string ("STEAM_X:Y:Z")
    pub guid: [c_char; SIGNED_GUID_LEN + 1], // +1 for null terminator

    /// Friend's SteamID64
    pub friends_id: u32,

    /// Friend's name
    pub friends_name: [c_char; MAX_PLAYER_NAME_LENGTH],

    /// Is this a fake player (bot)?
    pub fake_player: bool,

    /// Is this an HLTV bot/proxy?
    pub is_hltv: bool,

    /// Custom files downloaded
    pub custom_files: [u32; 4],
    pub files_downloaded: u8,

    _padding: [u8; 2],
}

impl Default for PlayerInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl PlayerInfo {
    /// Returns the player's name as a Rust String.
    pub fn name(&self) -> String {
        unsafe {
            CStr::from_ptr(self.name.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Returns the GUID (SteamID2) as a Rust String.
    pub fn guid(&self) -> String {
        unsafe {
            CStr::from_ptr(self.guid.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Returns the player's friend's name as a Rust String.
    pub fn friends_name(&self) -> String { // todo? what is this?
        unsafe {
            CStr::from_ptr(self.friends_name.as_ptr())
                .to_string_lossy()
                .into_owned()
        }
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CSteamID(pub u64);

impl CSteamID {
    /// Extracts the Account ID (the lower 32 bits of the SteamID64).
    pub fn account_id(&self) -> u32 {
        (self.0 & 0xFFFFFFFF) as u32 // todo? is this correct?
    }

    /// Returns `true` if the SteamID is valid (not zero).
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

#[repr(C)] pub struct SendTable { _private: [u8; 0] }
#[repr(C)] pub struct INetChannelInfo { _private: [u8; 0] }
#[repr(C)] pub struct CPlayerBitVec { _private: [u8; 0] }
#[repr(C)] pub struct CPlayerState { _private: [u8; 0] }
