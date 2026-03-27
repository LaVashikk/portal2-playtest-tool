use std::{ffi::{CStr, c_char, c_int, c_void}, fmt, ops::{Add, Mul, Sub}};
use crate::server_tools::IServerTools;

const MAX_PLAYER_NAME_LENGTH: usize = 128;
const SIGNED_GUID_LEN: usize = 32;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl fmt::Display for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(vector : ({}, {}, {}))", self.x, self.y, self.z)
    }
}

/// Overload for the + operator (Vector + Vector).
impl Add for Vector {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

/// Overload for the - operator (Vector - Vector).
impl Sub for Vector {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

/// Overload for the * operator (Vector * f32 scale).
impl Mul<f32> for Vector {
    type Output = Self;

    fn mul(self, scale: f32) -> Self {
        Self {
            x: self.x * scale,
            y: self.y * scale,
            z: self.z * scale,
        }
    }
}

impl Vector {
    /// Creates a new vector with the specified Cartesian coordinates.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Returns the distance from the origin.
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Returns the distance from the origin, ignoring the Z axis.
    pub fn length_2d(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns the distance from the origin, but squared.
    /// This is faster to compute since a square root isn't required.
    pub fn length_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Returns the distance from the origin, ignoring the Z axis and squared.
    pub fn length_2d_sqr(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Returns the distance between this vector and another.
    pub fn distance(&self, other: &Vector) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Returns the vector cross product (this x other).
    pub fn cross(&self, other: &Vector) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Returns the vector dot product (this . other).
    pub fn dot(&self, other: &Vector) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Modifies the vector to have a length of 1, and returns its original length.
    pub fn norm(&mut self) -> f32 {
        let len = self.length();
        if len != 0.0 {
            self.x /= len;
            self.y /= len;
            self.z /= len;
        }
        len
    }

    /// Returns a string in the form "X Y Z" (Equivalent to ToKVString).
    pub fn to_kv_string(&self) -> String {
        format!("{} {} {}", self.x, self.y, self.z)
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct QAngle {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl QAngle {
    /// Creates a new QAngle with the specified pitch, yaw, and roll.
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Converts the QAngle (Pitch and Yaw) into a forward directional Vector.
    /// This is the equivalent of the Source Engine's `AngleVectors` function
    pub fn to_forward_vector(&self) -> Vector {
        let pitch_rad = self.x.to_radians();
        let yaw_rad = self.y.to_radians();

        let (sin_pitch, cos_pitch) = pitch_rad.sin_cos();
        let (sin_yaw, cos_yaw) = yaw_rad.sin_cos();

        Vector {
            x: cos_pitch * cos_yaw,
            y: cos_pitch * sin_yaw,
            z: -sin_pitch, // iirc, in Source Engine, positive pitch looks down. Right?
        }
    }
}

impl fmt::Display for QAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(qangle : ({}, {}, {}))", self.x, self.y, self.z)
    }
}

impl From<QAngle> for Vector {
    fn from(angle: QAngle) -> Self {
        Self {
            x: angle.x,
            y: angle.y,
            z: angle.z,
        }
    }
}
impl From<Vector> for QAngle {
    fn from(vec: Vector) -> Self {
        Self {
            x: vec.x,
            y: vec.y,
            z: vec.z,
        }
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy)]
pub struct VMatrix {
    pub m: [[f32; 4]; 4],
}

/// A unique identifier for a networkable entity. It combines an entity index
/// with a serial number to prevent stale handles from referring to new entities
/// that have taken the same index.
#[repr(transparent)]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CBaseHandle(pub u32);


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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringT(pub u32);

/// Describes the network class of an entity.
#[repr(C)]
pub struct ServerClass {
    pub name: *const c_char,
    pub table: *mut SendTable,
    pub next: *mut ServerClass,
    pub class_id: c_int,
    pub instance_baseline_index: c_int,
}

impl ServerClass {
    /// Returns the network name of the class (e.g., "CPropPhysics", "CTerrorPlayer").
    pub fn get_name(&self) -> String {
        unsafe { CStr::from_ptr(self.name).to_string_lossy().into_owned() }
    }
}

// ==========================================================================
// IServerNetworkable
// ==========================================================================
#[repr(C)] pub struct IServerNetworkable { _private: [u8; 0] }

// uuuuuuuuuuuugh... im so lazy. so sorry
impl IServerNetworkable {
    // VTable index 0: GetEntityHandle
    // VTable index 1: GetServerClass
    // VTable index 2: GetEdict
    // VTable index 3: GetClassName
    // VTable index 4: Release
    // VTable index 5: AreaNum
    // VTable index 6: GetBaseNetworkable
    // VTable index 7: GetBaseEntity
    // VTable index 8: GetPVSInfo
    // VTable index 9: ~IServerNetworkable

    /// Returns the ServerClass associated with this entity.
    pub fn get_server_class<'a>(&self) -> Option<&'a mut ServerClass> {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_class: unsafe extern "thiscall" fn(*const IServerNetworkable) -> *mut ServerClass = std::mem::transmute(vtable.add(1).read());
            let ptr = get_class(self);
            if ptr.is_null() { None } else { Some(&mut *ptr) }
        }
    }

    /// Returns the network slot (Edict) attached to this entity.
    pub fn get_edict<'a>(&self) -> Option<&'a mut Edict> {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_edict: unsafe extern "thiscall" fn(*const IServerNetworkable) -> *mut Edict = std::mem::transmute(vtable.add(2).read());
            let ptr = get_edict(self);
            if ptr.is_null() { None } else { Some(&mut *ptr) }
        }
    }

    /// Returns the hardcoded C++ class name (e.g., "prop_dynamic").
    pub fn get_class_name(&self) -> String {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_name: unsafe extern "thiscall" fn(*const IServerNetworkable) -> *const c_char = std::mem::transmute(vtable.add(3).read());
            let ptr = get_name(self);
            if ptr.is_null() { String::new() } else { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
        }
    }
}


#[repr(C)] pub struct IServerEntity { _private: [u8; 0] }

impl IServerEntity {
    // VTable structure based on inheritance:
    // IHandleEntity:
    //   0: ~IHandleEntity
    //   1: SetRefEHandle
    //   2: GetRefEHandle
    // IServerUnknown:
    //   3: GetCollideable
    //   4: GetNetworkable
    //   5: GetBaseEntity
    // IServerEntity:
    //   0: ~IServerEntity (Overrides destructor, takes slot 0)
    //   6: GetModelIndex
    //   7: GetModelName
    //   8: SetModelIndex

    /// Returns the collision interface for this entity (bounding boxes, raycasts).
    pub fn get_collideable<'a>(&self) -> Option<&'a mut ICollideable> {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_col: unsafe extern "thiscall" fn(*const IServerEntity) -> *mut ICollideable = std::mem::transmute(vtable.add(3).read());
            let ptr = get_col(self);
            if ptr.is_null() { None } else { Some(&mut *ptr) }
        }
    }

    /// Returns the networkable interface (allows getting Edicts and ServerClasses).
    pub fn get_networkable<'a>(&self) -> Option<&'a mut IServerNetworkable> {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_net: unsafe extern "thiscall" fn(*const IServerEntity) -> *mut IServerNetworkable = std::mem::transmute(vtable.add(4).read());
            let ptr = get_net(self);
            if ptr.is_null() { None } else { Some(&mut *ptr) }
        }
    }

    /// Returns the model index of this entity.
    pub fn get_model_index(&self) -> i32 {
        unsafe {
            let vtable = *(self as *const _ as *const *const usize);
            let get_idx: unsafe extern "thiscall" fn(*const IServerEntity) -> c_int = std::mem::transmute(vtable.add(6).read());
            get_idx(self)
        }
    }
}

/// The core server-side entity class in the Source Engine.
#[repr(C)] pub struct CBaseEntity { _private: [u8; 0] }

impl CBaseEntity {
    /// Casts this entity to its `IServerEntity` interface safely.
    pub fn as_server_entity(&self) -> &IServerEntity {
        unsafe { &*(self as *const _ as *const IServerEntity) }
    }

    pub fn get_networkable(&self) -> Option<&mut IServerNetworkable> {
        self.as_server_entity().get_networkable()
    }

    /// Casts this entity to its mutable `IServerEntity` interface.
    pub fn as_server_entity_mut(&mut self) -> &mut IServerEntity {
        unsafe { &mut *(self as *mut _ as *mut IServerEntity) }
    }

    /// Shortcut: Gets the network Edict directly from the entity.
    pub fn get_edict<'a>(&self) -> Option<&'a mut Edict> {
        self.as_server_entity().get_networkable()?.get_edict()
    }

    /// Shortcut: Gets the C++ Class Name directly from the entity.
    pub fn get_class_name(&self) -> String {
        if let Some(net) = self.as_server_entity().get_networkable() {
            net.get_class_name()
        } else {
            String::new()
        }
    }

    /// Shortcut: Gets the Network Server Class directly.
    pub fn get_server_class<'a>(&self) -> Option<&'a mut ServerClass> {
        self.as_server_entity().get_networkable()?.get_server_class()
    }

    //
    // High-level entity manipulation methods
    //

    /// Retrieves the networkable class name of the entity.
    /// Returns an empty string if the entity is not networkable.
    pub fn get_classname(&self) -> String {
        if let Some(net) = self.get_networkable() {
            net.get_class_name()
        } else {
            String::new()
        }
    }

    /// Reads the current health of the entity via the engine's DataMap.
    pub fn get_health(&self, tools: &IServerTools) -> i32 {
        if let Some(val) = tools.get_key_value(self, "health") {
            val.parse().unwrap_or(0)
        } else {
            0
        }
    }

    /// Returns the current absolute world coordinates (origin) of the entity.
    pub fn get_origin(&self, tools: &IServerTools) -> Vector {
        if let Some(val) = tools.get_key_value(self, "origin") {
            // The string format is typically: "X Y Z"
            let parts: Vec<&str> = val.split_whitespace().collect();
            if parts.len() >= 3 {
                return Vector {
                    x: parts[0].parse().unwrap_or(0.0),
                    y: parts[1].parse().unwrap_or(0.0),
                    z: parts[2].parse().unwrap_or(0.0),
                };
            }
        }
        Vector::default()
    }

    /// Returns the rotation angles of the entity (Pitch, Yaw, Roll).
    pub fn get_angles(&self, tools: &IServerTools) -> QAngle {
        if let Some(val) = tools.get_key_value(self, "angles") {
            let parts: Vec<&str> = val.split_whitespace().collect();
            if parts.len() >= 3 {
                return QAngle {
                    x: parts[0].parse().unwrap_or(0.0),
                    y: parts[1].parse().unwrap_or(0.0),
                    z: parts[2].parse().unwrap_or(0.0),
                };
            }
        }
        QAngle::default()
    }

    /// Returns the target name ("targetname") of the entity.
    pub fn get_name(&self, tools: &IServerTools) -> String {
        tools.get_key_value(self, "targetname").unwrap_or_default()
    }

    /// Removes the entity from the world using IServerTools.
    pub fn destroy(&self, tools: &IServerTools) {
        if let Some(hammer_id_str) = tools.get_key_value(self, "hammerid") {
            if let Ok(hammer_id) = hammer_id_str.parse::<i32>() {
                tools.remove_entity(hammer_id);
            }
        }
    }

    /// Sets a string key-value field for the entity.
    pub fn set_key_value(&mut self, tools: &IServerTools, key: &str, value: &str) -> bool {
        tools.set_key_value_str(self, key, value)
    }

    /// Sets an integer key-value field for the entity.
    pub fn set_key_value_int(&mut self, tools: &IServerTools, key: &str, value: i32) -> bool {
        tools.set_key_value_flt(self, key, value as f32)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BBoxT {
    pub mins: Vector,
    pub maxs: Vector,
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

/// Holds information about an entity being respawned with edits (used by IServerTools).
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CEntityRespawnInfo {
    pub hammer_id: c_int,
    pub ent_text: *const c_char,
}

impl CEntityRespawnInfo {
    /// Safely retrieves the entity text (KeyValues block) as a Rust String.
    pub fn entity_text(&self) -> String {
        if self.ent_text.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(self.ent_text).to_string_lossy().into_owned() }
    }
}


/// Defines the underlying data type of a KeyValues node.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyValuesType {
    None = 0,
    String = 1,
    Int = 2,
    Float = 3,
    Ptr = 4,
    WString = 5,
    Color = 6,
    Uint64 = 7,
    CompiledIntByte = 8,
}

/// Union holding the raw value of a KeyValues node.
/// In a 32-bit Source Engine, this union occupies exactly 4 bytes.
#[repr(C)]
pub union KeyValuesValue {
    pub int_val: c_int,
    pub float_val: f32,
    pub ptr_val: *mut c_void,
    pub color_val: [u8; 4],
}

/// Represents the exact memory layout of the `KeyValues` C++ class in a 32-bit Source Engine.
/// This allows safe reading of parsed VDF (Valve Data Format) trees.
#[repr(C)]
pub struct KeyValues {
    pub key_name_symbol: u32,       // 0x00 CUtlSymbol (Identifier for the key name)
    pub string_value: *mut c_char,  // 0x04 Pointer to the string (if type is String)
    pub wstring_value: *mut u16,    // 0x08 Pointer to the wide string (if type is WString)
    pub value: KeyValuesValue,      // 0x0C The raw numeric/pointer value
    pub data_type: KeyValuesType,   // 0x10 The type of data stored in `value` or `string_value`
    pub has_escape_sequences: bool, // 0x11
    pub evaluate_conditionals: bool,// 0x12
    _pad: u8,                       // 0x13
    pub peer: *mut KeyValues,       // 0x14 Next element at the same hierarchy level
    pub sub: *mut KeyValues,        // 0x18 First child element (sub-key)
    pub chain: *mut KeyValues,      // 0x1C Chain element
}

impl KeyValues {
    /// Returns an Option referencing the next KeyValues node at the same level.
    pub fn next(&self) -> Option<&KeyValues> {
        unsafe { self.peer.as_ref() }
    }

    /// Returns an Option referencing the first child KeyValues node.
    pub fn first_sub_key(&self) -> Option<&KeyValues> {
        unsafe { self.sub.as_ref() }
    }

    /// Safely reads the value as a Rust String.
    /// If the node holds a number, it will be formatted as a string.
    pub fn get_string(&self) -> String {
        match self.data_type {
            KeyValuesType::String => unsafe {
                if self.string_value.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(self.string_value).to_string_lossy().into_owned()
                }
            },
            KeyValuesType::Int => unsafe { format!("{}", self.value.int_val) },
            KeyValuesType::Float => unsafe { format!("{:.4}", self.value.float_val) },
            _ => String::new(),
        }
    }

    /// Safely reads the value as an integer.
    /// If the node holds a float or a parsable string, it attempts to convert it.
    pub fn get_int(&self) -> i32 {
        match self.data_type {
            KeyValuesType::Int | KeyValuesType::CompiledIntByte => unsafe { self.value.int_val },
            KeyValuesType::Float => unsafe { self.value.float_val as i32 },
            KeyValuesType::String => self.get_string().parse().unwrap_or(0),
            _ => 0,
        }
    }

    /// Safely reads the value as a float.
    /// If the node holds an integer or a parsable string, it attempts to convert it.
    pub fn get_float(&self) -> f32 {
        match self.data_type {
            KeyValuesType::Float => unsafe { self.value.float_val },
            KeyValuesType::Int => unsafe { self.value.int_val as f32 },
            KeyValuesType::String => self.get_string().parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }
}

// TODO: Add comments explaining these opaque types.
#[repr(C)] pub struct SendTable { _private: [u8; 0] }
#[repr(C)] pub struct ModelT { _private: [u8; 0] }
#[repr(C)] pub struct ClientTextMessage { _private: [u8; 0] }
#[repr(C)] pub struct CSentence { _private: [u8; 0] }
#[repr(C)] pub struct CAudioSource { _private: [u8; 0] }
#[repr(C)] pub struct ISpatialQuery { _private: [u8; 0] }
#[repr(C)] pub struct IMaterialSystem { _private: [u8; 0] }
#[repr(C)] pub struct INetChannelInfo { _private: [u8; 0] }
#[repr(C)] pub struct IAchievementMgr { _private: [u8; 0] }
#[repr(C)] pub struct Edict { _private: [u8; 0] }
#[repr(C)] pub struct IClientEntity { _private: [u8; 0] }
#[repr(C)] pub struct IRecipientFilter { _private: [u8; 0] }
#[repr(C)] pub struct VPlane { _private: [u8; 0] }
#[repr(C)] pub struct PVSInfoT { _private: [u8; 0] }
#[repr(C)] pub struct ICollideable { _private: [u8; 0] }
#[repr(C)] pub struct ISpatialPartition { _private: [u8; 0] }
#[repr(C)] pub struct IScratchPad3D { _private: [u8; 0] }
#[repr(C)] pub struct CCheckTransmitInfo { _private: [u8; 0] }
#[repr(C)] pub struct CSharedEdictChangeInfo { _private: [u8; 0] }
#[repr(C)] pub struct IChangeInfoAccessor { _private: [u8; 0] }
#[repr(C)] pub struct ISPSharedMemory { _private: [u8; 0] }
#[repr(C)] pub struct CUtlVector { _private: [u8; 0] }
#[repr(C)] pub struct CBitVec { _private: [u8; 0] }
#[repr(C)] pub struct CPlayerBitVec { _private: [u8; 0] }
#[repr(C)] pub struct BfWrite { _private: [u8; 0] }
#[repr(C)] pub struct BfRead { _private: [u8; 0] }
#[repr(C)] pub struct CGamestatsData { _private: [u8; 0] }
#[repr(C)] pub struct CPlayerState { _private: [u8; 0] }
#[repr(C)] pub struct CSaveRestoreData { _private: [u8; 0] }
pub type QueryCvarCookieT = c_int;
pub type EQueryCvarValueStatus = c_int;
pub type SoundLevelT = c_int;
