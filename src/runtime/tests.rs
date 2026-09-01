use std::time::{Duration, UNIX_EPOCH};

    use super::{
        Value, coerce, default_span, host_random_seed, integer_from_i128_count, is_value,
        system_timestamp_ms,
    };
    use crate::semantic::{FloatType, IntegerType, Type};

    #[test]
    fn system_random_seed_is_never_zero() {
        assert_ne!(host_random_seed(), 0);
    }

    #[test]
    fn system_timestamp_before_epoch_is_negative() {
        assert_eq!(
            system_timestamp_ms(UNIX_EPOCH - Duration::from_millis(1)),
            -1
        );
    }

    #[test]
    fn integer_count_rejects_values_above_the_language_limit() {
        let error = integer_from_i128_count(i128::from(i32::MAX) + 1, default_span())
            .expect_err("INTEGER count overflow");
        assert_eq!(error.code, "NUMERIC_OVERFLOW");
    }

    #[test]
    fn network_handles_match_their_explicit_alternative_types() {
        let tcp = coerce(
            Value::TcpListener(1),
            &Type::Alternative(vec![
                Type::Named("HOST.Net.TCPListener".into()),
                Type::Named("Error".into()),
            ]),
            default_span(),
        );
        let udp = coerce(
            Value::UdpSocket(1),
            &Type::Alternative(vec![
                Type::Named("HOST.Net.UDPSocket".into()),
                Type::Named("Error".into()),
            ]),
            default_span(),
        );
        assert!(tcp.is_ok());
        assert!(udp.is_ok());
    }

    #[test]
    fn primitive_type_tests_match_their_runtime_values() {
        assert!(is_value(&Value::Integer(1, IntegerType::Int32), "INTEGER"));
        assert!(is_value(&Value::Integer(1, IntegerType::Byte), "BYTE"));
        assert!(!is_value(&Value::Integer(1, IntegerType::Byte), "INTEGER"));
        assert!(is_value(&Value::Float(1.0, FloatType::Float64), "FLOAT"));
        assert!(is_value(&Value::Boolean(true), "BOOLEAN"));
        assert!(is_value(&Value::String("BN".into()), "STRING"));
    }
