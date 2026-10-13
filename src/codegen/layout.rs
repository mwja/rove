use cranelift::prelude::Type;
use std::cmp::max;

#[derive(Debug, Clone)]
pub struct CalculatedField {
    pub offset: u32,
    pub size: u32,
    pub align: u32,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct StructLayout {
    pub fields: Vec<CalculatedField>,
    pub total_size: u32,
    pub total_align: u32,
}

/// Computes a C-repr memory layout for an array of Cranelift Types.
///
/// This could be further optimised (putting smaller aligned types near the
/// start so it uses less space) but for now I'll stick with C compat and the
/// defined order.
pub fn calculate_c_layout(field_types: &[Type]) -> StructLayout {
    let mut fields = Vec::with_capacity(field_types.len());
    let mut current_offset = 0u32;
    let mut max_alignment = 1u32;

    for &ty in field_types {
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
        });

        // 4. Advance past this field's data footprint
        current_offset += size;
    }

    // 5. Apply Trailing Padding: Pad the final struct size to match the widest member.
    let mut total_size = current_offset;
    if total_size % max_alignment != 0 {
        total_size = (total_size + max_alignment - 1) & !(max_alignment - 1);
    }

    StructLayout {
        fields,
        total_size,
        total_align: max_alignment,
    }
}
