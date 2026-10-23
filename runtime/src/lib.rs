//! Minimal runtime helpers for rove until it has its own formatting etc.

use std::{
    alloc::Layout,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Tracks number of live objects, mainly in debug modes to debug
/// leaks.
static LIVE: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_i64(value: i64) {
    println!("{}", value);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_f64(value: f64) {
    println!("{:?}", value);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_bool(value: bool) {
    println!("{:?}", value);
}

#[repr(u8)]
pub enum Constraint {
    Require = 0,
    Ensure = 1,
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_abort_constraint(
    kind: Constraint,
    tag: *const u8,
    tag_len: u32,
    fn_name: *const u8,
    fn_name_len: u32,
    line: u32,
) {
    // Safely construct slices using the provided lengths, then convert to lossy Cow<str>
    let tag = unsafe {
        let slice = std::slice::from_raw_parts(tag, tag_len as usize);
        String::from_utf8_lossy(slice)
    };

    let fn_name = unsafe {
        let slice = std::slice::from_raw_parts(fn_name, fn_name_len as usize);
        String::from_utf8_lossy(slice)
    };

    eprintln!(
        "fatal: {} constraint failed on function {}: tag={} line={}",
        match kind {
            Constraint::Require => "require",
            Constraint::Ensure => "ensure",
        },
        fn_name,
        tag,
        line
    );
    std::process::exit(1);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_abort_forced_try(fn_name: *const u8, fn_name_len: u32, line: u32) {
    let fn_name = unsafe {
        let slice = std::slice::from_raw_parts(fn_name, fn_name_len as usize);
        String::from_utf8_lossy(slice)
    };

    eprintln!(
        "fatal: try! performed on a function that errored, on function {}: line={}",
        fn_name, line
    );
    std::process::exit(1);
}

#[repr(C)]
pub struct TypeInfo {
    size: usize, // total allocation size incl. header
    align: usize,
    // Will have just array of u32s for offsets for now.
    // deinit: Option<extern "C" fn(*mut u8)>, // generated per struct, None if no ref fields
    ref_offsets: *const u32, // offsets of fields that are references, voidptr if none.
    ref_offsets_len: usize,
    name: *const u8,
    name_len: usize,
}

#[repr(C)]
pub struct Header {
    strong: u32,
    unowned: u32,
    weak: u32,
    flags: u32,
    type_info: *const TypeInfo,
}

/// Alloc a header of strong, unowned, and weak references. Store flags and tinfo.
///
/// This does NOT alloc the data segment
#[unsafe(no_mangle)]
pub extern "C" fn rt_alloc(type_info: *const TypeInfo) -> *mut u8 {
    LIVE.fetch_add(1, Ordering::SeqCst);

    unsafe {
        let ti = &*type_info;
        let layout = Layout::from_size_align(ti.size, ti.align).unwrap();
        let p = std::alloc::alloc_zeroed(layout); // zeroed: fields start null/0
        let h = p as *mut Header;
        (*h).strong = 1; // +1 returned to caller
        (*h).unowned = 0;
        (*h).weak = 0;
        (*h).flags = 0;
        (*h).type_info = type_info;
        p
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_retain(item_with_header: *const Header) {
    if !item_with_header.is_null() {
        unsafe {
            let ih = item_with_header as *mut Header;
            (*ih).strong += 1;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_release(item_with_header: *const Header) {
    if item_with_header.is_null() {
        return;
    }
    let ih = item_with_header as *mut Header;

    unsafe {
        debug_assert!((*ih).strong > 0);
        (*ih).strong -= 1;
        if (*ih).strong == 0 {
            let ti = (*ih).type_info;
            debug_assert!(!ti.is_null());

            for idx in 0..(*ti).ref_offsets_len {
                let off = *(*ti).ref_offsets.add(idx);
                // the as *mut u8 cast here is because otherwise it would add by the size of the Header. but the offset
                // is stored in bytes.
                let child = *((ih as *mut u8).add(off as usize) as *const *mut Header);
                // Release a referenced ref-counted value too
                rt_release(child);
            }
            let layout = Layout::from_size_align((*ti).size, (*ti).align).unwrap();
            std::alloc::dealloc(ih as *mut u8, layout);
            LIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

type RtEntryPoint = unsafe extern "C" fn() -> i64;

#[unsafe(no_mangle)]
pub extern "C" fn rt_start(entry_point: RtEntryPoint) -> i32 {
    let res = unsafe { entry_point() };

    // assuming it doesn't panic, stderr the live count if ROVE_DEBUG_LEAKS is set
    if std::env::var("ROVEFLAGS").map_or(false, |v| v.contains("debug_leaks")) {
        let live = LIVE.load(Ordering::SeqCst);
        eprintln!(
            "rove: live objects at exit: {} (remove debug_leaks from ROVEFLAGS to hide this message)",
            live
        );
    }

    res as i32
}

unsafe extern "C" {
    fn __rove_entry() -> i64;
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    rt_start(__rove_entry)
}
