//! Exception reporter: a first-chance vectored handler that logs access
//! violations and C++ exceptions with module+offset, the C++ type and
//! message, and a backtrace, so a crash leaves something readable in
//! `loader.log`. It never handles anything (always continues the search).
//!
//! C++ exceptions (`0xE06D7363`) are first-chance: the game, or a Rust
//! panic (which MSVC Rust raises the same way), may still catch them, so
//! they are logged as "may be caught". A foreign exception unwinding into a
//! mod's `catch_unwind` aborts the process with `0xC0000409` right after.

use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO, EXCEPTION_STACK_OVERFLOW, HMODULE};
use windows_sys::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, RtlCaptureStackBackTrace, EXCEPTION_POINTERS};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT};

use crate::log::log_error;

const MSVC_CPP_EXCEPTION: i32 = 0xE06D7363u32 as i32;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// Reports logged per session; the game may throw and catch on its own.
const MAX_REPORTS: u32 = 20;

static REPORTS: AtomicU32 = AtomicU32::new(0);

pub fn install() {
    unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let Some(rec) = info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) else {
        return EXCEPTION_CONTINUE_SEARCH;
    };
    let code = rec.ExceptionCode;
    let what = match code {
        EXCEPTION_ACCESS_VIOLATION => "access violation",
        EXCEPTION_ILLEGAL_INSTRUCTION => "illegal instruction",
        EXCEPTION_INT_DIVIDE_BY_ZERO => "integer divide by zero",
        EXCEPTION_STACK_OVERFLOW => "stack overflow",
        MSVC_CPP_EXCEPTION => "C++ exception (may be caught)",
        _ => return EXCEPTION_CONTINUE_SEARCH,
    };
    if REPORTS.fetch_add(1, Ordering::Relaxed) >= MAX_REPORTS {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // A stack overflow has no room for formatting and backtraces.
    if code == EXCEPTION_STACK_OVERFLOW {
        log_error!("exception: stack overflow at {}", location(rec.ExceptionAddress as usize));
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let mut detail = String::new();
    if code == EXCEPTION_ACCESS_VIOLATION && rec.NumberParameters >= 2 {
        let op = match rec.ExceptionInformation[0] {
            0 => "reading",
            1 => "writing",
            8 => "executing",
            _ => "accessing",
        };
        detail = format!(" ({op} {:#x})", rec.ExceptionInformation[1]);
    }
    if code == MSVC_CPP_EXCEPTION && rec.NumberParameters >= 4 {
        detail = cpp_exception_detail(rec.ExceptionInformation[1], rec.ExceptionInformation[2], rec.ExceptionInformation[3]);
    }
    let mut frames = [core::ptr::null_mut::<c_void>(); 32];
    let n = RtlCaptureStackBackTrace(0, frames.len() as u32, frames.as_mut_ptr(), core::ptr::null_mut()) as usize;
    // Drop this handler and the dispatcher (the first run of ntdll frames);
    // the walk continues through KiUserExceptionDispatcher into the
    // faulting frames.
    let all: Vec<String> = frames[..n].iter().map(|&f| location(f as usize)).collect();
    let is_ntdll = |l: &String| l.to_ascii_lowercase().starts_with("ntdll");
    let skip = all.iter().position(is_ntdll).map_or(0, |i| i + all[i..].iter().take_while(|l| is_ntdll(l)).count());
    log_error!("exception: {what}{detail} at {}", location(rec.ExceptionAddress as usize));
    log_error!("  stack: {}", all[skip..].join(" < "));
    EXCEPTION_CONTINUE_SEARCH
}

/// `module+0xoffset` for a code address.
fn location(addr: usize) -> String {
    let mut module: HMODULE = core::ptr::null_mut();
    let ok = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr as *const u16,
            &mut module,
        )
    };
    if ok == 0 || module.is_null() {
        return format!("{addr:#x}");
    }
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&buf[..len]);
    let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
    format!("{name}+{:#x}", addr - module as usize)
}

/// Type name and, for `std::exception`, the message of an MSVC C++
/// exception. x64 records: [1] object, [2] ThrowInfo RVA, [3] image base.
unsafe fn cpp_exception_detail(object: usize, throw_info: usize, image: usize) -> String {
    if throw_info == 0 || image == 0 {
        return String::new();
    }
    // ThrowInfo { attributes, pmfnUnwind, pForwardCompat, pCatchableTypeArray } (RVAs)
    let types = image + read_u32(throw_info + 12) as usize;
    let count = read_u32(types) as i32;
    if count <= 0 || types == image {
        return String::new();
    }
    // CatchableType { properties, pType, ... }; TypeDescriptor name at +0x10.
    let first = image + read_u32(types + 4) as usize;
    let type_desc = image + read_u32(first + 4) as usize;
    let name = c_str(type_desc + 0x10, 128);
    // Rust panics are raised as C++ exceptions of type "rust_panic".
    let is_std = (0..count as usize).any(|i| {
        let ct = image + read_u32(types + 4 + 4 * i) as usize;
        c_str(image + read_u32(ct + 4) as usize + 0x10, 64) == ".?AVexception@std@@"
    });
    // std::exception { vptr; __std_exception_data { const char* what; bool free } }
    let message = if is_std && object != 0 { c_str(*((object + 8) as *const usize), 256) } else { String::new() };
    if message.is_empty() {
        format!(" {name}")
    } else {
        format!(" {name}: \"{message}\"")
    }
}

unsafe fn read_u32(p: usize) -> u32 {
    core::ptr::read_unaligned(p as *const u32)
}

unsafe fn c_str(p: usize, max: usize) -> String {
    if p == 0 {
        return String::new();
    }
    let bytes: Vec<u8> = (0..max).map(|i| *((p + i) as *const u8)).take_while(|&b| b != 0).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_cpp_exceptions() {
        install();
        let before = REPORTS.load(Ordering::Relaxed);
        // MSVC Rust raises panics as C++ exceptions (type "rust_panic").
        let _ = std::panic::catch_unwind(|| panic!("test"));
        assert!(REPORTS.load(Ordering::Relaxed) > before);
        let s = location(reports_cpp_exceptions as *const () as usize);
        assert!(s.contains("+0x"), "{s}");
    }
}
