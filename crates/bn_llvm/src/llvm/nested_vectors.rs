// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The storage of fixed vectors of two or more dimensions (known issue K21):
// one block per binding with every row level, the elements and the
// dimensions, which `bn_rt_vector_nest` links and fills, so a copy takes the
// elements and never shares a row.
use bn_types::Type;

use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{handle_result_ty, llvm_type, status_pointer_ty, vector_ty};

/// A fixed vector of rank ≥ 2: its dimensions and LLVM element type.
pub(crate) struct Nest<'a> {
    dimensions: &'a [u64],
    element: T,
}

/// The nested shape of `ty`, when it is a fixed vector of two or more
/// dimensions.
pub(crate) fn nest(ty: &Type) -> Option<Nest<'_>> {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        return None;
    };
    let fixed = dimensions.len() >= 2 && !dimensions.contains(&u64::MAX);
    fixed.then_some(Nest {
        dimensions,
        element: llvm_type(element)?,
    })
}

impl Nest<'_> {
    fn rows_at(&self, level: usize) -> u64 {
        self.dimensions[..=level].iter().product()
    }

    fn rows(&self) -> u64 {
        (0..self.dimensions.len() - 1)
            .map(|l| self.rows_at(l))
            .sum()
    }

    fn leaves(&self) -> u64 {
        self.dimensions.iter().product()
    }

    /// `[K x i32]`: the type of the dimensions stored in the block.
    fn dimension_ty(&self) -> T {
        T::Array(self.dimensions.len(), Box::new(T::I32))
    }

    /// `[i32 2, i32 3]`: the dimensions as a constant.
    fn dimension_values(&self) -> O {
        O::array(
            self.dimensions
                .iter()
                .map(|&d| (T::I32, O::uint(d)))
                .collect(),
        )
    }

    /// Whether a `Default` with `dimensions` builds this shape: the same
    /// dimensions, each a row length, of scalar or reference elements.
    pub(crate) fn has_default(&self, dimensions: &[usize]) -> bool {
        let same =
            |(&own, &d): (&u64, &usize)| u32::try_from(own).is_ok() && u64::try_from(d) == Ok(own);
        let scalar = self.element.integer_bits().is_some_and(|bits| bits <= 64)
            || self.element.is_floating()
            || self.element == T::Ptr;
        let reference = [handle_result_ty(), status_pointer_ty()].contains(&self.element);
        (scalar || reference)
            && self.dimensions.len() == dimensions.len()
            && self.dimensions.iter().zip(dimensions).all(same)
    }

    /// The outermost length: the count of the binding's `{ ptr, i32 }`.
    pub(crate) fn count(&self) -> u64 {
        self.dimensions[0]
    }

    /// `{ rows, elements, dimensions }`: rows outermost level first.
    pub(crate) fn storage(&self) -> T {
        // A validated fixed vector has a length that fits the target's memory.
        let count = |n: u64| {
            let Ok(n) = usize::try_from(n) else {
                unreachable!()
            };
            n
        };
        T::Struct(vec![
            T::Array(count(self.rows()), Box::new(vector_ty())),
            T::Array(count(self.leaves()), Box::new(self.element.clone())),
            self.dimension_ty(),
        ])
    }

    /// The constant of a global storage named `global`, its rows already
    /// linked: a `STATIC` is readable before its first store.
    pub(crate) fn linked_constant(&self, global: &str) -> String {
        let storage = self.storage();
        let at = |field: u64, index: u64| {
            format!(
                "ptr getelementptr inbounds ({storage}, ptr {global}, i32 0, i32 {field}, i64 {index})"
            )
        };
        let levels = self.dimensions.len() - 1;
        let mut rows = Vec::new();
        let mut next_start = 0;
        for level in 0..levels {
            next_start += self.rows_at(level);
            let length = self.dimensions[level + 1];
            for index in 0..self.rows_at(level) {
                let child = index * length;
                let target = if level + 1 == levels {
                    at(1, child)
                } else {
                    at(0, next_start + child)
                };
                rows.push(format!("{{ ptr, i32 }} {{ {target}, i32 {length} }}"));
            }
        }
        format!(
            "{{ [{} x {{ ptr, i32 }}] [{}], [{} x {}] zeroinitializer, {} {} }}",
            self.rows(),
            rows.join(", "),
            self.leaves(),
            self.element,
            self.dimension_ty(),
            self.dimension_values()
        )
    }

    /// The default value `v{destination}`: a vector of zero elements (`""` for
    /// `STRING`, as a scalar `STRING` defaults) in a storage block of its own.
    pub(crate) fn emit_default(
        &self,
        text: &mut crate::ir::LlvmModule,
        destination: u32,
        strings: bool,
    ) {
        let storage = self.storage();
        let base = O::reg(format!("vecdefault{destination}"));
        text.assign(
            format!("vecdefault{destination}"),
            I::alloca(storage.clone()),
        );
        text.emit(I::store(
            storage.clone(),
            O::zero_initializer(),
            base.clone(),
        ));
        self.emit_copy(text, &base, O::null());
        for index in (0..self.leaves()).filter(|_| strings) {
            let at = vec![
                (T::I32, O::int(0)),
                (T::I32, O::int(1)),
                (T::I64, O::uint(index)),
            ];
            let leaf = format!("vecdefaultleaf{destination}_{index}");
            text.assign(&leaf, I::gep(storage.clone(), base.clone(), at));
            text.emit(I::store(T::Ptr, O::global(".bn_empty"), O::reg(leaf)));
        }
        let first = format!("vecdefaultfat{destination}");
        text.assign(&first, I::insert(vector_ty(), O::undef(), T::Ptr, base, 0));
        let count = O::uint(self.count());
        let fat = I::insert(vector_ty(), O::reg(first), T::I32, count, 1);
        text.assign(format!("v{destination}"), fat);
    }

    /// Links the storage at `target` and copies into it the elements of the
    /// vector whose rows start at `source` (`null`: no copy).
    pub(crate) fn emit_copy(&self, text: &mut crate::ir::LlvmModule, target: &O, source: O) {
        let tag = text.fresh_tag();
        let storage = self.storage();
        let field = |index: i64| vec![(T::I32, O::int(0)), (T::I32, O::int(index))];
        text.assign(
            format!("vnleaves{tag}"),
            I::gep(storage.clone(), target.clone(), field(1)),
        );
        text.assign(
            format!("vndims{tag}"),
            I::gep(storage, target.clone(), field(2)),
        );
        let dims = O::reg(format!("vndims{tag}"));
        let values = self.dimension_values();
        text.emit(I::store(self.dimension_ty(), values, dims.clone()));
        let size = I::gep(self.element.clone(), O::null(), vec![(T::I32, O::int(1))]);
        text.assign(format!("vnsize{tag}"), size);
        let bytes = I::cast(
            CastOp::PtrToInt,
            T::Ptr,
            O::reg(format!("vnsize{tag}")),
            T::I64,
        );
        text.assign(format!("vnbytes{tag}"), bytes);
        let args = vec![
            (T::Ptr, target.clone()),
            (T::Ptr, O::reg(format!("vnleaves{tag}"))),
            (T::Ptr, source),
            (T::Ptr, dims),
            (T::I32, O::uint(self.dimensions.len() as u64)),
            (T::I64, O::reg(format!("vnbytes{tag}"))),
        ];
        text.emit(I::call(T::Void, "bn_rt_vector_nest", args));
    }
}
