use std::ffi::c_int;

// TODO: Add comments explaining these opaque types.
#[repr(C)] pub struct ModelT { _private: [u8; 0] }
#[repr(C)] pub struct ClientTextMessage { _private: [u8; 0] }
#[repr(C)] pub struct CSentence { _private: [u8; 0] }
#[repr(C)] pub struct CAudioSource { _private: [u8; 0] }
#[repr(C)] pub struct ISpatialQuery { _private: [u8; 0] }
#[repr(C)] pub struct IMaterialSystem { _private: [u8; 0] }
#[repr(C)] pub struct IAchievementMgr { _private: [u8; 0] }
#[repr(C)] pub struct IRecipientFilter { _private: [u8; 0] }
#[repr(C)] pub struct PVSInfoT { _private: [u8; 0] }
#[repr(C)] pub struct ISPSharedMemory { _private: [u8; 0] }

pub type QueryCvarCookieT = c_int;
pub type EQueryCvarValueStatus = c_int;
pub type SoundLevelT = c_int;
