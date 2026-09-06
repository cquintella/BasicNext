use bn_value::{Handle, Value};

#[test]
fn value_leaf_carries_scalars_vectors_and_runtime_handles() {
    let handle = Handle::new(0, 0);
    let value = Value::Vector(vec![Value::Integer(7, bn_types::IntegerType::Int32)]);
    assert!(matches!(value, Value::Vector(_)));
    assert!(matches!(Value::Pointer { handle }, Value::Pointer { .. }));
}
