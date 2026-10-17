//! Helpers for the in-memory `hyperdemon.exe` image: sections, IAT patching,
//! byte-pattern scanning.

use core::ffi::c_void;

use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

#[derive(Clone, Copy)]
pub struct Image {
    pub base: usize,
    pub size: usize,
}

unsafe fn rd<T: Copy>(a: usize) -> T {
    core::ptr::read_unaligned(a as *const T)
}

impl Image {
    pub fn main_module() -> Image {
        unsafe {
            let base = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(core::ptr::null()) as usize;
            let nt = base + rd::<u32>(base + 0x3c) as usize;
            let size = rd::<u32>(nt + 24 + 56) as usize; // OptionalHeader.SizeOfImage
            Image { base, size }
        }
    }

    fn nt(&self) -> usize {
        self.base + unsafe { rd::<u32>(self.base + 0x3c) } as usize
    }

    /// PE TimeDateStamp, logged to identify the game build.
    pub fn timestamp(&self) -> u32 {
        unsafe { rd(self.nt() + 8) }
    }

    /// `(start, len)` of the named section.
    pub fn section(&self, name: &str) -> Option<(usize, usize)> {
        unsafe {
            let nt = self.nt();
            let count = rd::<u16>(nt + 6) as usize;
            let opt_size = rd::<u16>(nt + 20) as usize;
            let first = nt + 24 + opt_size;
            for i in 0..count {
                let s = first + i * 40;
                let raw: [u8; 8] = rd(s);
                let len = raw.iter().position(|&b| b == 0).unwrap_or(8);
                if &raw[..len] == name.as_bytes() {
                    let vsize = rd::<u32>(s + 8) as usize;
                    let va = rd::<u32>(s + 12) as usize;
                    return Some((self.base + va, vsize));
                }
            }
            None
        }
    }

    pub fn text(&self) -> &'static [u8] {
        let (start, len) = self.section(".text").unwrap_or((self.base, self.size));
        unsafe { core::slice::from_raw_parts(start as *const u8, len) }
    }

    /// Address of the first exact occurrence of `needle` in `.rdata`.
    pub fn find_rdata(&self, needle: &[u8]) -> Option<usize> {
        let (start, len) = self.section(".rdata")?;
        let hay = unsafe { core::slice::from_raw_parts(start as *const u8, len) };
        hay.windows(needle.len()).position(|w| w == needle).map(|i| start + i)
    }

    /// Address of the IAT slot through which this image imports `func` from `dll`.
    pub fn iat_slot(&self, dll: &str, func: &str) -> Option<*mut usize> {
        unsafe {
            let nt = self.nt();
            let import_rva = rd::<u32>(nt + 24 + 112 + 8) as usize; // DataDirectory[1]
            if import_rva == 0 {
                return None;
            }
            let mut desc = self.base + import_rva;
            loop {
                let ilt = rd::<u32>(desc) as usize;
                let name = rd::<u32>(desc + 12) as usize;
                let iat = rd::<u32>(desc + 16) as usize;
                if name == 0 {
                    return None;
                }
                if cstr_at(self.base + name).eq_ignore_ascii_case(dll) {
                    let lookup = if ilt != 0 { ilt } else { iat };
                    let mut i = 0;
                    loop {
                        let entry = rd::<u64>(self.base + lookup + i * 8);
                        if entry == 0 {
                            break;
                        }
                        if entry >> 63 == 0 && cstr_at(self.base + entry as usize + 2) == func {
                            return Some((self.base + iat + i * 8) as *mut usize);
                        }
                        i += 1;
                    }
                    return None;
                }
                desc += 20;
            }
        }
    }
}

unsafe fn cstr_at(a: usize) -> &'static str {
    std::ffi::CStr::from_ptr(a as *const i8).to_str().unwrap_or("")
}

/// Swap an IAT entry, returning the previous function pointer.
pub unsafe fn patch_iat(slot: *mut usize, new: usize) -> usize {
    let mut old = 0u32;
    VirtualProtect(slot as *const c_void, 8, PAGE_READWRITE, &mut old);
    let prev = core::ptr::replace(slot, new);
    VirtualProtect(slot as *const c_void, 8, old, &mut old);
    prev
}

/// Parsed IDA-style pattern: `None` is a wildcard byte.
pub fn parse_pattern(p: &str) -> Option<Vec<Option<u8>>> {
    p.split_whitespace()
        .map(|t| match t {
            "?" | "??" => Some(None),
            _ => u8::from_str_radix(t, 16).ok().map(Some),
        })
        .collect()
}

/// All offsets in `hay` where `pat` matches.
pub fn scan(hay: &[u8], pat: &[Option<u8>]) -> Vec<usize> {
    let Some(anchor) = pat.iter().position(|b| b.is_some()) else { return Vec::new() };
    let first = pat[anchor].unwrap();
    let mut hits = Vec::new();
    if hay.len() < pat.len() {
        return hits;
    }
    for i in 0..=hay.len() - pat.len() {
        if hay[i + anchor] != first {
            continue;
        }
        if pat.iter().enumerate().all(|(k, b)| b.is_none_or(|b| hay[i + k] == b)) {
            hits.push(i);
        }
    }
    hits
}
