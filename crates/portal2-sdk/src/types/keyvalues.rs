use std::ffi::{CStr, c_char, c_int, c_void};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringT(pub u32);

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

#[repr(C)] pub struct CUtlVector { _private: [u8; 0] }
#[repr(C)] pub struct CBitVec { _private: [u8; 0] }
#[repr(C)] pub struct BfWrite { _private: [u8; 0] }
#[repr(C)] pub struct BfRead { _private: [u8; 0] }
#[repr(C)] pub struct CGamestatsData { _private: [u8; 0] }
#[repr(C)] pub struct CSaveRestoreData { _private: [u8; 0] }
