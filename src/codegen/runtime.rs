//! Declarations of the functions provided by the rove runtime.

use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct Runtime {
    /// rt_println_i64(value:i64) -> ()
    pub(super) println_i64: FuncId,
    /// rt_println_f64(value:f64) -> ()
    pub(super) println_f64: FuncId,
    /// rt_println_bool(value:i8) -> ()
    pub(super) println_bool: FuncId,
    /// rt_println_str(value:ptr) -> ()
    pub(super) println_str: FuncId,
    /// rt_println_string(value:ptr) -> ()
    pub(super) println_string: FuncId,
    /// rt_abort_constraint(kind:u8, tag:*u8, tag_len:u32, fn_name:*u8, fn_name_len:u32, line:u32) -> !
    pub(super) abort_constraint: FuncId,
    /// rt_abort_forced_try(fn_name:*u8, fn_name_len:u32, line:u32) -> !
    pub(super) abort_forced_try: FuncId,
    /// rt_start(ptr) -> i32
    pub(super) start: FuncId,
    // for now type info is never passed as I don't currently have rt_release, but once passed it'll be useful
    /// rt_alloc(tinfo: ptr TypeInfo) -> ptr
    pub(super) alloc: FuncId,
    /// rt_retain(item: ptr Header) -> ()
    pub(super) retain: FuncId,
    /// rt_release(item: ptr Header) -> ()
    pub(super) release: FuncId,
}

impl Runtime {
    pub fn from_object(module: &mut ObjectModule) -> Self {
        Self {
            println_i64: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::I64));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_i64", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            println_f64: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::F64));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_f64", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            println_bool: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::I8));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_bool", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            println_str: {
                let mut printf_sig = module.make_signature();

                printf_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                module
                    .declare_function("rt_println_str", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            println_string: {
                let mut printf_sig = module.make_signature();

                printf_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                module
                    .declare_function("rt_println_string", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            abort_constraint: {
                let mut abort_sig = module.make_signature();

                abort_sig.params.push(AbiParam::new(types::I8));
                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig.params.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_abort_constraint", Linkage::Import, &abort_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            abort_forced_try: {
                let mut abort_sig = module.make_signature();

                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig.params.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_abort_forced_try", Linkage::Import, &abort_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            start: {
                let mut start_sig = module.make_signature();

                start_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                start_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_start", Linkage::Import, &start_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            alloc: {
                let mut alloc_sig = module.make_signature();

                alloc_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                alloc_sig
                    .returns
                    .push(AbiParam::new(module.target_config().pointer_type()));

                module
                    .declare_function("rt_alloc", Linkage::Import, &alloc_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            retain: {
                let mut alloc_sig = module.make_signature();

                alloc_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                module
                    .declare_function("rt_retain", Linkage::Import, &alloc_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            release: {
                let mut alloc_sig = module.make_signature();

                alloc_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                module
                    .declare_function("rt_release", Linkage::Import, &alloc_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
        }
    }
}
