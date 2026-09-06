//! Minimal runtime helpers for rove until it has its own formatting etc.

#[unsafe(no_mangle)]
pub extern "C" fn rt_println_i64(value: i64) {
    println!("{}", value);
}
