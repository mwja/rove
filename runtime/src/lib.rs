//! Minimal runtime helpers for rove until it has its own formatting etc.

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
    pub size: usize, // total allocation size incl. header
    pub align: usize,
    pub deinit: Option<extern "C" fn(*mut u8)>, // generated per struct, None if no ref fields
    pub name: *const u8,                        // optional: for leak reports
    pub name_len: usize,
}

#[repr(C)]
struct Header {
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
pub extern "C" fn rt_alloc(size: usize, type_info: *const TypeInfo) -> *mut u8 {
    use std::alloc::Layout;

    unsafe {
        let layout = Layout::from_size_align(size, 16).unwrap();
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

type RtEntryPoint = unsafe extern "C" fn() -> i64;

#[unsafe(no_mangle)]
pub extern "C" fn rt_start(entry_point: RtEntryPoint) -> i32 {
    let res = unsafe { entry_point() };

    res as i32
}

unsafe extern "C" {
    fn __rove_entry() -> i64;
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    rt_start(__rove_entry)
}
