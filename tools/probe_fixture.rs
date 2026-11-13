// SPDX-License-Identifier: MPL-2.0
// Original, redistributable probe-only fixture. No game code, ROM or firmware.
// Implements only the metadata ABI consumed by Engine CoreProbe v2, not emulation.
use std::ffi::c_char;

#[repr(C)]
pub struct SystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
pub struct AytherInterfaceV1 {
    abi_version: u32,
    struct_size: u32,
    capabilities: u64,
    host_endianness: u32,
    pointer_size: u32,
    region_info_size: u32,
    frame_snapshot_size: u32,
    sprite_size: u32,
    audio_write_size: u32,
    build_id: *const c_char,
    build_id_size: u32,
    reserved0: u32,
}

unsafe impl Sync for AytherInterfaceV1 {}

static BUILD_ID: &[u8] = b"Play probe fixture ABI 1.10\0";

static INTERFACE: AytherInterfaceV1 = AytherInterfaceV1 {
    abi_version: (1 << 16) | 10,
    struct_size: std::mem::size_of::<AytherInterfaceV1>() as u32,
    capabilities: 0xEFFFF,
    host_endianness: 0x0102_0304,
    pointer_size: std::mem::size_of::<usize>() as u32,
    region_info_size: 0,
    frame_snapshot_size: 0,
    sprite_size: 0,
    audio_write_size: 0,
    build_id: BUILD_ID.as_ptr().cast::<c_char>(),
    build_id_size: (BUILD_ID.len() - 1) as u32,
    reserved0: 0,
};

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

#[unsafe(no_mangle)]
pub extern "C" fn ayther_get_interface(_requested_version: u32) -> *const AytherInterfaceV1 {
    &raw const INTERFACE
}
