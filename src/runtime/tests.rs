use std::time::{Duration, UNIX_EPOCH};

use std::{fs, sync::atomic::{AtomicU64, Ordering}};

    use super::{
        Value, coerce, default_span, host_random_seed, integer_from_i128_count, is_value,
    };
    use crate::semantic::{FloatType, IntegerType, Type};

    #[test]
    fn web_callback_uses_a_fresh_executor_and_projects_response() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "basicnext-web-callback-{}-{}.bn",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source = r#"IMPORT BNWeb AS Web
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(207)
response.Write("isolated")
END FUNCTION
FUNCTION Start() AS VOID
END FUNCTION
"#;
        fs::write(&path, source).expect("write callback fixture");
        let graph = crate::module_graph::load(path.to_str().expect("fixture path"))
            .expect("load callback fixture");
        let models = crate::semantic::analyze_modules(&graph).expect("analyze callback fixture");
        let module = crate::ir::lower_graph(&graph, &models).expect("lower callback fixture");
        let request = crate::web::Request::new(
            "GET",
            "/callback",
            Vec::new(),
            "",
            "127.0.0.1".parse().expect("peer address"),
        )
        .expect("construct callback request");
        let response = super::execute_web_callback(
            &module,
            &super::HostEnv::fixed(vec!["callback.bn".into()], 0, 0),
            "Handler",
            request,
            crate::web::Response::new(),
        )
        .expect("execute callback");
        let _ = fs::remove_file(path);
        assert_eq!(response.status, 207);
        assert_eq!(response.body, "isolated");
    }

    #[test]
    fn system_random_seed_is_never_zero() {
        assert_ne!(host_random_seed(), 0);
    }

    #[test]
    fn async_host_forks_receive_independent_random_states() {
        let host = super::HostEnv::fixed(vec!["async.bn".into()], 0, 0);
        let first = host.fork_for_task();
        let second = host.fork_for_task();
        assert_ne!(
            first.random_state.load(Ordering::Relaxed),
            second.random_state.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn async_tasks_do_not_cross_talk_through_host_random_state() {
        let host = super::HostEnv::fixed(vec!["async.bn".into()], 0, 0);
        let first = host.fork_for_task();
        let second = host.fork_for_task();
        let first_values = std::thread::spawn(move || {
            [
                first.random_state.fetch_add(1, Ordering::Relaxed),
                first.random_state.fetch_add(1, Ordering::Relaxed),
            ]
        });
        let second_values = std::thread::spawn(move || {
            [
                second.random_state.fetch_add(1, Ordering::Relaxed),
                second.random_state.fetch_add(1, Ordering::Relaxed),
            ]
        });
        let first_values = first_values.join().expect("first task");
        let second_values = second_values.join().expect("second task");
        assert_eq!(first_values[1], first_values[0] + 1);
        assert_eq!(second_values[1], second_values[0] + 1);
        assert_ne!(first_values[0], second_values[0]);
    }

    #[test]
    fn system_timestamp_before_epoch_is_negative() {
        assert_eq!(
            bn_rt::timestamp_ms_from(UNIX_EPOCH - Duration::from_millis(1)),
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
