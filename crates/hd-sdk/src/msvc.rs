//! Reading and writing the game's MSVC `std::string` (32 bytes:
//! `union { char buf[16]; char* ptr; }`, `size_t size`, `size_t capacity`;
//! the buffer is inline while `capacity < 16`).
//!
//! Writes go through the game's own `std::string::assign`, so any heap memory
//! is allocated and later freed by the game's allocator.

use crate::mem::read;

const SIZE: usize = 0x10;
const CAPACITY: usize = 0x18;

/// Copies the string at `addr` out of game memory.
///
/// # Safety
/// `addr` must point to a live MSVC `std::string`.
pub unsafe fn read_string(addr: usize) -> String {
    let size: usize = read(addr + SIZE);
    let cap: usize = read(addr + CAPACITY);
    let data = if cap > 15 { read::<usize>(addr) } else { addr };
    if data == 0 || size > (1 << 20) {
        return String::new();
    }
    String::from_utf8_lossy(core::slice::from_raw_parts(data as *const u8, size)).into_owned()
}

/// Replaces the contents of the `std::string` at `addr`. Returns `false` if
/// the game's `assign` was not resolved.
///
/// # Safety
/// `addr` must point to a live MSVC `std::string` owned by the game.
pub unsafe fn assign_string(addr: usize, value: &str) -> bool {
    let f = crate::addrs().string_assign;
    if f == 0 {
        return false;
    }
    type AssignFn = unsafe extern "C" fn(s: usize, p: *const u8, n: usize) -> usize;
    core::mem::transmute::<usize, AssignFn>(f)(addr, value.as_ptr(), value.len());
    true
}
