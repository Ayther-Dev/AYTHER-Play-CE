// SPDX-License-Identifier: MPL-2.0
// Original, redistributable probe-only fixture. No game code, ROM or firmware.
// Implements only the metadata ABI consumed by Engine CoreProbe, not emulation.
use std::ffi::c_char;

#[repr(C)]
pub struct SystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[unsafe(no_mangle)]
pub extern "C" fn retro_api_version() -> u32 {
    1
}

/// # Safety
/// `info` must point to a writable Libretro system-info structure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn retro_get_system_info(info: *mut SystemInfo) {
    // SAFETY: CoreProbe supplies a valid pointer to the C ABI structure.
    unsafe {
        info.write(SystemInfo {
            library_name: c"Play probe fixture".as_ptr(),
            library_version: c"1".as_ptr(),
            valid_extensions: c"bin".as_ptr(),
            need_fullpath: false,
            block_extract: false,
        });
    }
}
