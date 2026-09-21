use bn_value::{Handle, RecordValue, Value, shared_string};

#[test]
fn value_leaf_carries_scalars_vectors_and_runtime_handles() {
    let handle = Handle::new(0, 0);
    let value = Value::Vector(vec![Value::Integer(7, bn_types::IntegerType::Int32)]);
    assert!(matches!(value, Value::Vector(_)));
    assert!(matches!(Value::Pointer { handle }, Value::Pointer { .. }));
}

#[test]
fn cloned_string_values_share_immutable_storage() {
    let original = Value::String(shared_string("shared immutable text"));
    let cloned = original.clone();

    let (Value::String(original_text), Value::String(cloned_text)) = (&original, &cloned) else {
        panic!("the values must remain strings");
    };

    assert_eq!(original_text, cloned_text);
    assert!(std::ptr::eq(original_text.as_ptr(), cloned_text.as_ptr()));
}

#[test]
fn positional_records_have_checked_ordered_slot_access() {
    let mut record = RecordValue::new(
        "Point",
        vec![
            Value::Integer(3, bn_types::IntegerType::Int32),
            Value::Boolean(true),
        ],
    );

    assert_eq!(record.type_name().as_ref(), "Point");
    assert_eq!(record.len(), 2);
    assert!(!record.is_empty());
    assert!(matches!(record.get(0), Some(Value::Integer(3, _))));
    assert!(record.get(2).is_none());
    assert!(record.get_mut(2).is_none());
    assert!(record.replace(2, Value::Null).is_none());
    assert!(matches!(
        record.replace(1, Value::Null),
        Some(Value::Boolean(true))
    ));
    assert_eq!(record.iter().len(), 2);
    assert_eq!(record.into_fields().len(), 2);
}

#[test]
fn value_representation_fits_the_64_bit_budget() {
    if cfg!(target_pointer_width = "64") {
        assert!(
            std::mem::size_of::<Value>() <= 48,
            "Value is {} bytes",
            std::mem::size_of::<Value>()
        );
    }
}
