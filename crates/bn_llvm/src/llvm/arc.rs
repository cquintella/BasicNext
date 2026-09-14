use super::*;

pub(crate) fn is_class_type(module: &Module, ty: &Type) -> bool {
    let Some(name) = class_name(ty) else {
        return false;
    };
    !is_struct_type(module, ty)
        && module
            .functions
            .iter()
            .any(|function| function.name == format!("{name}.$fields"))
}

pub(crate) fn class_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(name) => Some(name.clone()),
        Type::ImportedNamed { module, name } => Some(format!("#{}.{name}", module.0)),
        Type::Alternative(alternatives)
            if alternatives.len() == 2
                && alternatives.iter().any(|item| matches!(item, Type::Null)) =>
        {
            alternatives.iter().find_map(|item| match item {
                Type::Named(name) => Some(name.clone()),
                Type::ImportedNamed { module, name } => Some(format!("#{}.{name}", module.0)),
                _ => None,
            })
        }
        _ => None,
    }
}

pub(crate) fn destructor_symbol(module: &Module, ty: &Type) -> Option<String> {
    let destructor = format!("{}.DESTRUCTOR", class_name(ty)?);
    module
        .functions
        .iter()
        .any(|function| function.name == destructor)
        .then(|| llvm_function_symbol(&destructor))
}

pub(crate) fn runtime_ir() -> &'static str {
    r"
@bn_arc_weak_objects = private global [4096 x ptr] zeroinitializer
@bn_arc_weak_locations = private global [4096 x ptr] zeroinitializer
@bn_arc_weak_lock = private global i32 0

define void @bn_arc_weak_lock_acquire() {
entry:
  br label %attempt
attempt:
  %old = atomicrmw xchg ptr @bn_arc_weak_lock, i32 1 acquire
  %acquired = icmp eq i32 %old, 0
  br i1 %acquired, label %done, label %attempt
done:
  ret void
}

define void @bn_arc_weak_lock_release() {
entry:
  store atomic i32 0, ptr @bn_arc_weak_lock release, align 4
  ret void
}

define void @bn_arc_weak_unregister(ptr %location) {
entry:
  call void @bn_arc_weak_lock_acquire()
  br label %scan
scan:
  %index = phi i64 [ 0, %entry ], [ %next, %continue ]
  %slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_locations, i64 0, i64 %index
  %registered = load ptr, ptr %slot
  %match = icmp eq ptr %registered, %location
  br i1 %match, label %clear, label %continue
clear:
  %object_slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_objects, i64 0, i64 %index
  store ptr null, ptr %object_slot
  store ptr null, ptr %slot
  br label %continue
continue:
  %next = add i64 %index, 1
  %done = icmp eq i64 %next, 4096
  br i1 %done, label %exit, label %scan
exit:
  call void @bn_arc_weak_lock_release()
  ret void
}

define void @bn_arc_weak_register(ptr %object, ptr %location) {
entry:
  call void @bn_arc_weak_unregister(ptr %location)
  %nonnull = icmp ne ptr %object, null
  br i1 %nonnull, label %lock, label %done
lock:
  call void @bn_arc_weak_lock_acquire()
  br label %scan
scan:
  %index = phi i64 [ 0, %lock ], [ %next, %occupied ]
  %slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_locations, i64 0, i64 %index
  %registered = load ptr, ptr %slot
  %empty = icmp eq ptr %registered, null
  br i1 %empty, label %insert, label %occupied
insert:
  %object_slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_objects, i64 0, i64 %index
  store ptr %object, ptr %object_slot
  store ptr %location, ptr %slot
  call void @bn_arc_weak_lock_release()
  ret void
occupied:
  %next = add i64 %index, 1
  %full = icmp eq i64 %next, 4096
  br i1 %full, label %overflow, label %scan
overflow:
  call void @bn_arc_weak_lock_release()
  call void @exit(i32 1)
  unreachable
done:
  ret void
}

define void @bn_arc_weak_invalidate(ptr %object) {
entry:
  call void @bn_arc_weak_lock_acquire()
  br label %scan
scan:
  %index = phi i64 [ 0, %entry ], [ %next, %continue ]
  %object_slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_objects, i64 0, i64 %index
  %registered = load ptr, ptr %object_slot
  %match = icmp eq ptr %registered, %object
  br i1 %match, label %clear, label %continue
clear:
  %location_slot = getelementptr [4096 x ptr], ptr @bn_arc_weak_locations, i64 0, i64 %index
  %location = load ptr, ptr %location_slot
  store ptr null, ptr %location
  store ptr null, ptr %object_slot
  store ptr null, ptr %location_slot
  br label %continue
continue:
  %next = add i64 %index, 1
  %done = icmp eq i64 %next, 4096
  br i1 %done, label %exit, label %scan
exit:
  call void @bn_arc_weak_lock_release()
  ret void
}

define void @bn_arc_retain(ptr %object) {
entry:
  %nonnull = icmp ne ptr %object, null
  br i1 %nonnull, label %retain, label %done
retain:
  %countptr = getelementptr i8, ptr %object, i64 8
  %count = load i64, ptr %countptr
  %next = add i64 %count, 1
  store i64 %next, ptr %countptr
  br label %done
done:
  ret void
}

define i1 @bn_arc_release(ptr %object) {
entry:
  %nonnull = icmp ne ptr %object, null
  br i1 %nonnull, label %release, label %done
release:
  %countptr = getelementptr i8, ptr %object, i64 8
  %count = load i64, ptr %countptr
  %valid = icmp ugt i64 %count, 0
  br i1 %valid, label %decrement, label %invalid
decrement:
  %next = sub i64 %count, 1
  store i64 %next, ptr %countptr
  %last = icmp eq i64 %next, 0
  ret i1 %last
invalid:
  call void @exit(i32 1)
  unreachable
done:
  ret i1 false
}
"
}

#[allow(clippy::only_used_in_recursion)]
pub(crate) fn emit_destroy_if_last(
    text: &mut String,
    module: &Module,
    function: &Function,
    object: &str,
    ty: &Type,
    symbols: &HashMap<SymbolId, usize>,
    state: &mut EmissionState,
) {
    let n = state.continuation_count;
    state.continuation_count += 1;
    let last = format!("arc_last{n}");
    let destroy = format!("arc_destroy{n}");
    let next = format!("arc_next{n}");
    let _ = writeln!(text, "  %{last} = call i1 @bn_arc_release(ptr {object})");
    let _ = writeln!(text, "  br i1 %{last}, label %{destroy}, label %{next}");
    state.control_flow.label(text, destroy);
    let _ = writeln!(text, "  call void @bn_arc_weak_invalidate(ptr {object})");
    if let Some(destructor) = destructor_symbol(module, ty) {
        let _ = writeln!(text, "  call void @{destructor}(ptr {object})");
    }
    if let Some(owner) = class_name(ty) {
        for (declaring, name, field_ty) in class_layout_fields(module, &owner) {
            if !is_class_type(module, &field_ty)
                || module
                    .weak_fields
                    .contains(&(declaring.clone(), name.clone()))
            {
                continue;
            }
            let offset = field_byte_offset(module, &declaring, &name);
            let field_ptr = format!("%arcfieldptr{n}_{offset}");
            let field_object = format!("%arcfieldobj{n}_{offset}");
            let _ = writeln!(
                text,
                "  {field_ptr} = getelementptr i8, ptr {object}, i32 {offset}"
            );
            let _ = writeln!(text, "  {field_object} = load ptr, ptr {field_ptr}");
            emit_destroy_if_last(
                text,
                module,
                function,
                &field_object,
                &field_ty,
                symbols,
                state,
            );
        }
    }
    let _ = writeln!(text, "  call void @free(ptr {object})");
    let _ = writeln!(text, "  br label %{next}");
    state.control_flow.label(text, next);
}
