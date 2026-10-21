use cranelift::prelude::Type;
use std::cmp::max;

use crate::{codegen::repr::ReprCx, structs::Struct, ty::TyKind};

#[derive(Debug, Clone)]
pub struct CalculatedField {
    pub offset: u32,
    pub size: u32,
    pub align: u32,
    pub ty: Type,
    /// Is this field a refcounted pointer that also has to be rt_release'd
    pub managed: bool,
}

#[derive(Debug, Clone)]
pub struct StructLayout {
    pub fields: Vec<CalculatedField>,
    pub total_size: u32,
    pub total_align: u32,
}

const HEADER_SIZE: u32 = 24; // strong,unowned,weak,flags (4×u32) + type_info ptr = 4*4+8

/// Computes a C-repr memory layout for an array of Cranelift Types.
///
/// This could be further optimised (putting smaller aligned types near the
/// start so it uses less space) but for now I'll stick with C compat and the
/// defined order.
///
/// I need to do better C-interop though, as Rove structs store a header which
/// offsets the fields by 24 bytes and messes with the alignment. A runtime
/// interop function could be interesting.
fn calculate_c_layout(field_types: &[(Type, bool)]) -> StructLayout {
    let mut fields = Vec::with_capacity(field_types.len());
    let mut current_offset = HEADER_SIZE;
    let mut max_alignment = 8u32; // ptr width

    for &(ty, managed) in field_types {
        // 1. In C, the natural alignment of a primitive type equals its byte size.
        let size = ty.bytes();
        let align = size; // Default natural alignment

        // Track the maximum alignment needed for trailing padding
        max_alignment = max(max_alignment, align);

        // 2. Align the current offset upward to match the field's required alignment.
        if current_offset % align != 0 {
            current_offset = (current_offset + align - 1) & !(align - 1);
        }

        // 3. Commit the field layout
        fields.push(CalculatedField {
            offset: current_offset,
            size,
            align,
            ty,
            managed,
        });

        // 4. Advance past this field's data footprint
        current_offset += size;
    }

    // 5. Apply Trailing Padding: Pad the final struct size to match the widest member.
    let mut total_size = current_offset;
    if !total_size.is_multiple_of(max_alignment) {
        total_size = (total_size + max_alignment - 1) & !(max_alignment - 1);
    }

    StructLayout {
        fields,
        total_size,
        total_align: max_alignment,
    }
}

/// Calculates the layout of the given struct
pub fn calculate_layout(rcx: &ReprCx, struct_: &Struct) -> StructLayout {
    let types = struct_
        .fields
        .iter()
        .map(|f| {
            let ty = rcx.repr_of(&f.ty).expect_scalar("...");
            (ty, matches!(f.ty.kind(), TyKind::Struct(_)))
        })
        .collect::<Vec<_>>();

    calculate_c_layout(&types)
}
