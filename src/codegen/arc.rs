//! Reference counting: retain/release and runtime type info.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    /// Lowers tinfo (TypeInfo) for the runtime into a ptr to a data section
    pub(super) fn get_cached_tinfo(&mut self, struct_: &Struct) -> GlobalValue {
        if let Some(tinfo) = self.cached_tinfo.get(&struct_.id) {
            return self.module.declare_data_in_func(*tinfo, self.builder.func);
        }

        // Structure is (on 64 bit platforms)
        // 0..    8..   16..       24..        32.. 40..     48..56
        // +------+-----+----------+-----------+----+--------+----+
        // | size |align|ref_offset|ref_off_len|name|name_len|drop|
        // +------+-----+----------+-----------+----+--------+----+
        // usize. usize. *u32.      usize.      *u8. usize.   fn ptr (null for structs)

        let layout = self.get_cached_layout(struct_.id);
        let ptr_ty = self.module.target_config().pointer_type();
        let ptr_bytes = ptr_ty.bytes() as usize;
        let usize_le = |v: u64| v.to_le_bytes()[..ptr_bytes].to_vec();

        let ref_offsets: Vec<u32> = layout
            .fields
            .iter()
            .filter(|f| f.managed) // see note 1
            .map(|f| f.offset)
            .collect();
        let ref_offsets_len = ref_offsets.len();

        let name_data = self.define_anon(struct_.name.as_bytes().into(), false, false);
        let roffset_data = (ref_offsets_len > 0)
            .then(|| self.define_anon(iter_u32_to_u8(ref_offsets.into_iter()), false, false));

        // size | align | ref_offsets | ref_offsets_len | name | name_len | drop
        // pointer slots are left as zero then are filed in with the relocations (below)
        let mut bytes = Vec::with_capacity(ptr_bytes * 7);
        bytes.extend(usize_le(layout.total_size as u64));
        bytes.extend(usize_le(layout.total_align as u64));
        bytes.extend(usize_le(0)); // ref_offsets (reloc, or stays null)
        bytes.extend(usize_le(ref_offsets_len as u64));
        bytes.extend(usize_le(0)); // name (reloc)
        bytes.extend(usize_le(struct_.name.len() as u64));
        bytes.extend(usize_le(0)); // drop (structs need none, their ref fields are released by offset)

        let mut desc = DataDescription::new();
        desc.define(bytes.into_boxed_slice());
        desc.set_align(ptr_bytes as u64);
        if let Some(roffset_data) = roffset_data {
            let gv = desc.import_global_value(roffset_data.into());
            desc.write_data_addr((ptr_bytes * 2) as u32, gv, 0);
        }
        let name_gv = desc.import_global_value(name_data.into());
        desc.write_data_addr((ptr_bytes * 4) as u32, name_gv, 0);

        let tinfo = self
            .module
            .declare_anonymous_data(false, false)
            .unwrap_bug("unable to declare anonymous data for tinfo");
        self.module
            .define_data(tinfo, &desc)
            .unwrap_bug("unable to define data desc for tinfo");

        self.cached_tinfo.insert(struct_.id, tinfo);
        self.module.declare_data_in_func(tinfo, self.builder.func)
    }

    /// Shortcut to define anonymous dta when you don't care about the name.
    pub(super) fn define_anon(&mut self, data: Box<[u8]>, writeable: bool, tls: bool) -> DataId {
        let data_id = self
            .module
            .declare_anonymous_data(writeable, tls)
            .unwrap_or_else(|e| bug!("unable to declare anonymous data: {e}"));
        let mut data_desc = DataDescription::new();
        data_desc.define(data);
        self.module
            .define_data(data_id, &data_desc)
            .unwrap_or_else(|e| bug!("unable to define data: {e}"));
        data_id
    }

    pub(super) fn emit_retain(&mut self, v: Value) {
        let func_ref = self.get_or_cache_function(self.runtime.retain);
        self.builder.ins().call(func_ref, &[v]);
    }

    pub(super) fn emit_release(&mut self, v: Value) {
        let func_ref = self.get_or_cache_function(self.runtime.release);
        self.builder.ins().call(func_ref, &[v]);
    }

    pub(super) fn as_owned(&mut self, v: Operand, own: Ownership, ty: &Ty) -> Operand {
        if ty.is_ref_counted() && own == Borrowed {
            self.emit_retain(v.expect_scalar("can't ref count non-scalar"));
        }
        v
    }

    pub(super) fn as_borrowed(&mut self, v: Operand, own: Ownership, ty: &Ty) -> Operand {
        if ty.is_ref_counted() && own == Owned {
            self.temps
                .push(v.expect_scalar("can't ref count non-scalar"));
        }
        v
    }
}
