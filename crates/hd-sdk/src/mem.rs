//! Raw memory helpers for poking at game structures.
//!
//! These are thin wrappers over unaligned pointer reads/writes. They do not
//! validate addresses: reading a dangling game pointer will crash the game, as
//! it would in C++.

/// # Safety
/// `addr` must point to readable memory holding a valid `T`.
#[inline]
pub unsafe fn read<T: Copy>(addr: usize) -> T {
    core::ptr::read_unaligned(addr as *const T)
}

/// # Safety
/// `addr` must point to writable memory where a `T` belongs.
#[inline]
pub unsafe fn write<T: Copy>(addr: usize, value: T) {
    core::ptr::write_unaligned(addr as *mut T, value)
}

/// Dereferences `base`, then follows each offset as a pointer except the
/// last, which is added to produce the final address. Returns `None` on a null
/// link. `chain(p, &[0x10, 0x8])` is `*(*(p) + 0x10) + 0x8`.
///
/// # Safety
/// Every non-null intermediate pointer must be readable.
pub unsafe fn chain(base: usize, offsets: &[usize]) -> Option<usize> {
    let mut addr: usize = read(base);
    if addr == 0 {
        return None;
    }
    for (i, off) in offsets.iter().enumerate() {
        if i + 1 == offsets.len() {
            return Some(addr + off);
        }
        addr = read(addr + off);
        if addr == 0 {
            return None;
        }
    }
    Some(addr)
}

/// Absolute address of an RVA inside `hyperdemon.exe`.
pub fn rva(offset: usize) -> usize {
    crate::host().game_base + offset
}

/// Finds an IDA-style byte pattern (`"48 8B 05 ?? ?? ?? ??"`) in the game's
/// code section.
pub fn find_pattern(pattern: &str) -> Option<usize> {
    let r = unsafe { (crate::host().find_pattern)(crate::abi::StrRef::of(pattern)) };
    (r != 0).then_some(r)
}

/// Resolves a `rip`-relative operand: `insn` is the instruction address,
/// `disp_offset` the offset of its 32-bit displacement and `insn_len` its
/// total length.
///
/// # Safety
/// `insn` must point to readable code.
pub unsafe fn rip_relative(insn: usize, disp_offset: usize, insn_len: usize) -> usize {
    let disp: i32 = read(insn + disp_offset);
    (insn + insn_len).wrapping_add_signed(disp as isize)
}
