// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Fixed vectors of two or more dimensions in compiled programs (0.6.md,
//! value semantics; known issue K21): links the rows of a binding's own
//! storage and copies the elements of another vector of the same shape into
//! it, so no binding shares a row with another.

/// One vector row as `bnc` lays it out: LLVM `{ ptr, i32 }`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Row {
    elements: *mut u8,
    length: i32,
}

/// The shape of a nested vector: its dimensions and element size.
struct Shape<'a> {
    dimensions: &'a [u32],
    element_bytes: usize,
}

impl Shape<'_> {
    /// The rows of level `level`: the product of the dimensions up to it.
    fn rows_at(&self, level: usize) -> usize {
        self.dimensions[..=level]
            .iter()
            .map(|&dimension| dimension as usize)
            .product()
    }

    /// The bytes of one innermost row.
    fn leaf_row_bytes(&self) -> usize {
        self.dimensions[self.dimensions.len() - 1] as usize * self.element_bytes
    }
}

/// Points every row of `rows` (all row levels, outermost first) at the next
/// level, and the innermost rows at consecutive runs of `leaves`.
#[allow(unsafe_code)] // Writes rows inside the caller's storage.
fn link(rows: *mut Row, leaves: *mut u8, shape: &Shape<'_>) {
    let levels = shape.dimensions.len() - 1;
    let mut level_start = 0;
    for level in 0..levels {
        let count = shape.rows_at(level);
        let next_start = level_start + count;
        let length = shape.dimensions[level + 1];
        for index in 0..count {
            let child = index * length as usize;
            // SAFETY: `rows` holds the rows of every level (`bnc` sizes the
            // storage from the same dimensions) and `leaves` every element;
            // `level_start + index` and `next_start + child` stay inside the
            // rows, `child * element_bytes` inside the elements.
            unsafe {
                let elements = if level + 1 == levels {
                    leaves.add(child * shape.element_bytes)
                } else {
                    rows.add(next_start + child).cast::<u8>()
                };
                rows.add(level_start + index).write(Row {
                    elements,
                    length: i32::try_from(length).unwrap_or(i32::MAX),
                });
            }
        }
        level_start = next_start;
    }
}

/// Copies the elements under `source` (rows of level `level`) to `cursor`,
/// in row order, and returns the position after them.
#[allow(unsafe_code)] // Reads the source vector's rows and elements.
fn copy_leaves(source: *const Row, level: usize, cursor: *mut u8, shape: &Shape<'_>) -> *mut u8 {
    let mut cursor = cursor;
    for index in 0..shape.dimensions[level] as usize {
        // SAFETY: `source` is a vector of the same fixed shape (the IR is
        // validated), so it has `dimensions[level]` rows at this level and
        // each innermost row `leaf_row_bytes` of elements; `ptr::copy`
        // allows the regions to overlap.
        unsafe {
            let row = source.add(index).read();
            if level + 2 == shape.dimensions.len() {
                std::ptr::copy(row.elements, cursor, shape.leaf_row_bytes());
                cursor = cursor.add(shape.leaf_row_bytes());
            } else {
                // An outer row points at the next row level, which `link` laid out
                // as `Row`s: the cast keeps their alignment.
                #[allow(clippy::cast_ptr_alignment)]
                let inner = row.elements.cast::<Row>();
                cursor = copy_leaves(inner, level + 1, cursor, shape);
            }
        }
    }
    cursor
}

/// Makes `rows` and `leaves` (the storage of a fixed vector of `rank ≥ 2`
/// dimensions `dimensions`, elements of `element_bytes`) a vector whose rows
/// point into that storage, then copies the elements of `source` (the
/// outermost rows of a vector of the same shape) into it. A null `source`,
/// or the storage itself, copies nothing.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_vector_nest(
    rows: *mut Row,
    leaves: *mut u8,
    source: *const Row,
    dimensions: *const u32,
    rank: u32,
    element_bytes: u64,
) {
    if rows.is_null() || dimensions.is_null() || rank < 2 {
        return;
    }
    // SAFETY: `bnc` passes `rank` dimensions in a stack array that lives
    // for the call.
    let dimensions = unsafe { std::slice::from_raw_parts(dimensions, rank as usize) };
    let shape = Shape {
        dimensions,
        element_bytes: usize::try_from(element_bytes).unwrap_or(0),
    };
    link(rows, leaves, &shape);
    if !source.is_null() && source != rows.cast_const() {
        copy_leaves(source, 0, leaves, &shape);
    }
}

#[cfg(test)]
mod tests {
    use super::{Row, bn_rt_vector_nest};

    const DIMENSIONS: [u32; 2] = [2, 3];
    const EMPTY: Row = Row {
        elements: std::ptr::null_mut(),
        length: 0,
    };

    /// Links `rows` and `leaves` as a 2×3 vector of `i64`, copying `source`.
    fn nest(rows: &mut [Row; 2], leaves: &mut [i64; 6], source: *const Row) {
        bn_rt_vector_nest(
            rows.as_mut_ptr().cast(),
            leaves.as_mut_ptr().cast(),
            source,
            DIMENSIONS.as_ptr(),
            2,
            8,
        );
    }

    #[test]
    fn rows_point_into_their_own_storage() {
        let (mut rows, mut leaves) = ([EMPTY; 2], [1, 2, 3, 4, 5, 6]);
        nest(&mut rows, &mut leaves, std::ptr::null());
        assert_eq!(rows[0].length, 3);
        assert_eq!(rows[1].elements.cast_const(), leaves[3..].as_ptr().cast());
        assert_eq!(leaves, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn a_copy_takes_the_elements_not_the_rows() {
        let (mut source_rows, mut source_leaves) = ([EMPTY; 2], [1, 2, 3, 4, 5, 6]);
        nest(&mut source_rows, &mut source_leaves, std::ptr::null());
        let (mut rows, mut leaves) = ([EMPTY; 2], [0; 6]);
        nest(&mut rows, &mut leaves, source_rows.as_ptr().cast());
        assert_eq!(leaves, [1, 2, 3, 4, 5, 6]);
        assert_eq!(rows[0].elements.cast_const(), leaves.as_ptr().cast());
        assert_ne!(rows[0].elements, source_rows[0].elements);
    }

    #[test]
    fn copying_a_vector_onto_itself_keeps_it() {
        let (mut rows, mut leaves) = ([EMPTY; 2], [1, 2, 3, 4, 5, 6]);
        nest(&mut rows, &mut leaves, std::ptr::null());
        let own = rows.as_ptr().cast();
        nest(&mut rows, &mut leaves, own);
        assert_eq!(leaves, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn three_dimensions_link_every_level() {
        let dimensions = [2_u32, 2, 2];
        let mut rows = [EMPTY; 6];
        let mut leaves = [0_i32; 8];
        bn_rt_vector_nest(
            rows.as_mut_ptr().cast(),
            leaves.as_mut_ptr().cast(),
            std::ptr::null(),
            dimensions.as_ptr(),
            3,
            4,
        );
        assert_eq!(rows[1].elements.cast_const(), rows[4..].as_ptr().cast());
        assert_eq!(rows[5].elements.cast_const(), leaves[6..].as_ptr().cast());
    }
}
