/// Aborts compilation due to an internal compiler error (an invariant that should never be
/// violated). Accepts the same arguments as [`format!`].
#[macro_export]
macro_rules! bug {
    ($($arg:tt)*) => {
        panic!("internal compiler error: {}", format_args!($($arg)*))
    };
}
