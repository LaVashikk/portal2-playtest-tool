use std::ffi::c_char;
use std::sync::OnceLock;
use crate::interfaces;
use crate::cvar::Color;

type FnMsg = unsafe extern "C" fn(format: *const c_char, msg: *const c_char);
type FnConColorMsg = unsafe extern "C" fn(clr: *const Color, format: *const c_char, msg: *const c_char);

fn get_msg_fn() -> Option<FnMsg> {
    static MSG_FN: OnceLock<Option<FnMsg>> = OnceLock::new();
    *MSG_FN.get_or_init(|| unsafe {
        interfaces::find_export(b"tier0.dll\0", b"Msg\0")
            .or_else(|| interfaces::find_export(b"tier0.dll\0", b"?Msg@@YAXPBDZZ\0"))
            .map(|ptr| std::mem::transmute(ptr))
    })
}

fn get_con_color_msg_fn() -> Option<FnConColorMsg> {
    static CON_COLOR_MSG_FN: OnceLock<Option<FnConColorMsg>> = OnceLock::new();
    *CON_COLOR_MSG_FN.get_or_init(|| unsafe {
        interfaces::find_export(b"tier0.dll\0", b"ConColorMsg\0")
            .or_else(|| interfaces::find_export(b"tier0.dll\0", b"?ConColorMsg@@YAXABVColor@@PBDZZ\0"))
            .map(|ptr| std::mem::transmute(ptr))
    })
}

fn get_warning_fn() -> Option<FnMsg> {
    static WARNING_FN: OnceLock<Option<FnMsg>> = OnceLock::new();
    *WARNING_FN.get_or_init(|| unsafe {
        interfaces::find_export(b"tier0.dll\0", b"Warning\0")
            .or_else(|| interfaces::find_export(b"tier0.dll\0", b"?Warning@@YAXPBDZZ\0"))
            .map(|ptr| std::mem::transmute(ptr))
    })
}

/// Sends a high-level system message via `tier0.dll` (`Msg`).
pub fn msg(message: &str) {
    if let Some(func) = get_msg_fn() {
        if let Ok(c_msg) = std::ffi::CString::new(message) {
            let fmt = b"%s\0".as_ptr() as *const c_char;
            unsafe { func(fmt, c_msg.as_ptr()) };
        }
    }
}

/// Sends a high-level colored message via `tier0.dll` (`ConColorMsg`).
pub fn con_color_msg(color: Color, message: &str) {
    if let Some(func) = get_con_color_msg_fn() {
        if let Ok(c_msg) = std::ffi::CString::new(message) {
            let fmt = b"%s\0".as_ptr() as *const c_char;
            unsafe { func(&color as *const Color, fmt, c_msg.as_ptr()) };
        }
    }
}

/// Sends a system warning message via `tier0.dll` (`Warning`).
pub fn warning(message: &str) {
    if let Some(func) = get_warning_fn() {
        if let Ok(c_msg) = std::ffi::CString::new(message) {
            let fmt = b"%s\0".as_ptr() as *const c_char;
            unsafe { func(fmt, c_msg.as_ptr()) };
        }
    }
}
