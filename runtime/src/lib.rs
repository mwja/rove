//! Minimal runtime helpers for rove until it has its own formatting etc.

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_i64(value: i64) {
    println!("{}", value);
}

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_f64(value: f64) {
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
