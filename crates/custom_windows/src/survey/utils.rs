
use windows::Win32::System::LibraryLoader::GetModuleHandleExA;
use windows::Win32::System::LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};
use windows::Win32::Foundation::HMODULE;

pub fn get_dll_directory() -> Option<std::path::PathBuf> {
    unsafe {
        let mut hmodule = HMODULE::default();

        let result = GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCSTR(get_dll_directory as *const u8),
            &mut hmodule,
        );

        if result.is_err() {
            return None;
        }

        let mut path_buf = vec![0u8; 512];
        let len = windows::Win32::System::LibraryLoader::GetModuleFileNameA(
            Some(hmodule),
            &mut path_buf,
        );

        if len == 0 {
            return None;
        }

        let path_str = std::str::from_utf8(&path_buf[..len as usize]).ok()?;
        let dll_path = std::path::PathBuf::from(path_str);
        dll_path.parent().map(|p| p.to_path_buf())
    }
}

pub fn show_error_and_panic(caption: &str, text: &str) -> ! {
    use std::ffi::CString;
    use windows::core::PCSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_OK, MB_ICONERROR};

    log::error!("ERROR {}: {}", caption, text);
    let lp_text = CString::new(text).unwrap();
    let lp_caption = CString::new(caption).unwrap();
    unsafe {
        MessageBoxA(
            None,
            PCSTR(lp_text.as_ptr() as *const u8),
            PCSTR(lp_caption.as_ptr() as *const u8),
            MB_OK | MB_ICONERROR,
        );
    }
    panic!("{}", text);
}
