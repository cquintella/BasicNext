// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
#[test]
fn bndispatch_queue_rejects_worker_count_outside_the_bounded_range() {
    let source = r"IMPORT BNDispatch AS Dispatch
FUNCTION Start() AS VOID
LET invalid AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(0)
LET valid AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(1)
PRINT invalid IS Error, valid IS Error
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute BNDispatch queue limits");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn bndispatch_ticket_lifecycle_is_bounded() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = queue.Async(Work)
IF ticket IS Error THEN
PRINT "error"
ELSE
PRINT ticket.Status(), ticket.Cancel(), ticket.IsDone()
LET closed AS VOID OR Error = queue.Close(100)
PRINT closed IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch ticket lifecycle");
    assert!(
        [
            "0 TRUE TRUE\nFALSE\n",
            "0 FALSE FALSE\nFALSE\n",
            "1 FALSE FALSE\nFALSE\n",
            "1 FALSE TRUE\nFALSE\n",
            "2 FALSE TRUE\nFALSE\n"
        ]
        .contains(&output.as_str()),
        "unexpected ticket race outcome: {output:?}"
    );
}

#[test]
fn async_await_syntax_dispatches_through_bounded_queue_api() {
    let source = r#"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
PRINT "work"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "queue-error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()
IF ticket IS Error THEN
PRINT "ticket-error"
ELSE
LET result AS VOID OR Error = AWAIT ticket(1000)
PRINT result IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("ASYNC/AWAIT dispatch syntax");
    assert!(
        output == "work\nFALSE\n" || output == "FALSE\n" || output == "work TRUE\n",
        "unexpected output: {output:?}"
    );
}

#[test]
fn typed_async_await_returns_worker_integer_result() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION PiPart(i AS INTEGER) AS INTEGER OR Error
RETURN i + 1
END FUNCTION
FUNCTION Start() AS VOID OR Error
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN queue
END IF
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue PiPart(3)
IF ticket IS Error THEN
RETURN ticket
END IF
LET result AS INTEGER OR Error = AWAIT ticket(1000)
IF result IS Error THEN
RETURN result
END IF
PRINT result
RETURN ticket.Close()
END FUNCTION
";
    let (_, output) = run(source, "").expect("typed AWAIT should return worker value");
    assert_eq!(output, "4\n");
}

#[test]
fn typed_async_await_supports_mvp_scalars_and_replay() {
    let source = r#"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION MakeFloat() AS FLOAT OR Error
RETURN 1.5
END FUNCTION
ASYNC FUNCTION MakeString() AS STRING OR Error
RETURN "ok"
END FUNCTION
ASYNC FUNCTION MakeBoolean() AS BOOLEAN OR Error
RETURN TRUE
END FUNCTION
FUNCTION Start() AS VOID OR Error
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN queue
END IF
LET tf AS Dispatch.Ticket OR Error = ASYNC queue MakeFloat()
LET ts AS Dispatch.Ticket OR Error = ASYNC queue MakeString()
LET tb AS Dispatch.Ticket OR Error = ASYNC queue MakeBoolean()
IF tf IS Error OR ts IS Error OR tb IS Error THEN
RETURN
END IF
LET f AS FLOAT OR Error = AWAIT tf(1000)
LET s AS STRING OR Error = AWAIT ts(1000)
LET b AS BOOLEAN OR Error = AWAIT tb(1000)
LET f_again AS FLOAT OR Error = AWAIT tf(1000)
IF f IS Error OR s IS Error OR b IS Error OR f_again IS Error THEN
RETURN
END IF
PRINT f, s, b, f_again
tf.Close()
ts.Close()
tb.Close()
RETURN queue.Close(1000)
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("typed scalar AWAIT should replay values");
    assert_eq!(output, "1.5 ok TRUE 1.5\n");
}

#[test]
fn typed_async_submission_passes_worker_arguments() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Add(left AS INTEGER, right AS INTEGER) AS INTEGER OR Error
RETURN left + right
END FUNCTION
FUNCTION Start() AS VOID OR Error
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN queue
END IF
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Add(2, 3)
IF ticket IS Error THEN
RETURN ticket
END IF
LET result AS INTEGER OR Error = AWAIT ticket(1000)
IF result IS Error THEN
RETURN result
END IF
PRINT result
RETURN queue.Close(1000)
END FUNCTION
";
    let (_, output) = run(source, "").expect("typed async submission should pass worker arguments");
    assert_eq!(output, "5\n");
}

#[test]
fn typed_async_await_rejects_runtime_result_type_mismatch() {
    let source = r#"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Text() AS STRING OR Error
RETURN "wrong"
END FUNCTION
FUNCTION Start() AS VOID OR Error
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN queue
END IF
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Text()
IF ticket IS Error THEN
RETURN ticket
END IF
LET result AS INTEGER OR Error = AWAIT ticket(1000)
PRINT result IS Error
RETURN queue.Close(1000)
END FUNCTION
"#;
    let error = run(source, "").expect_err("mismatched typed AWAIT should be rejected");
    assert_eq!(error.code, "TYPE_MISMATCH");
}

#[test]
fn async_task_output_overflow_fails_ticket_without_retaining_partial_output() {
    let output_source = (0..7_000)
        .map(|_| "PRINT \"0123456789\"\n")
        .collect::<String>();
    let source = format!(
        "IMPORT BNDispatch AS Dispatch\nASYNC FUNCTION Work() AS VOID\n{output_source}END FUNCTION\nFUNCTION Start() AS VOID\nLET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()\nIF queue IS Error THEN\nPRINT \"queue-error\"\nELSE\nLET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()\nIF ticket IS Error THEN\nPRINT \"ticket-error\"\nELSE\nLET result AS VOID OR Error = ticket.Wait(1000)\nPRINT result IS Error, ticket.Status()\nEND IF\nEND IF\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("execute bounded async output");
    assert_eq!(output, "TRUE 3\n");
}

#[test]
fn async_function_requires_error_alternative_for_typed_return_type() {
    let source = "ASYNC FUNCTION Invalid() AS INTEGER\nRETURN 1\nEND FUNCTION\nFUNCTION Start() AS VOID\nEND FUNCTION\n";
    let error = run(source, "").expect_err("typed ASYNC return must include Error");
    assert_eq!(error.code, "ASYNC_RETURN_TYPE");
}

#[test]
fn async_submission_rejects_an_invalid_target_operand() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN
ELSE
LET target AS INTEGER = 1
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue target
END IF
END FUNCTION
";
    let error = run(source, "").expect_err("literal ASYNC target must be rejected");
    assert_eq!(error.code, "E0100");
}

#[test]
fn await_rejects_literal_timeout_outside_contract() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN
ELSE
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()
IF ticket IS Error THEN
RETURN
ELSE
LET result AS VOID OR Error = AWAIT ticket(0)
END IF
END IF
END FUNCTION
";
    let error = run(source, "").expect_err("invalid AWAIT timeout");
    assert_eq!(error.code, "AWAIT_TIMEOUT");
}

#[test]
fn bndispatch_sync_primitives_have_bounded_operations() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Start() AS VOID
LET group AS Dispatch.Group OR Error = Dispatch.Group.New()
LET barrier AS Dispatch.Barrier OR Error = Dispatch.Barrier.New(1)
LET semaphore AS Dispatch.Semaphore OR Error = Dispatch.Semaphore.New(1)
LET mutex AS Dispatch.Mutex OR Error = Dispatch.Mutex.New()
IF group IS Error OR barrier IS Error OR semaphore IS Error OR mutex IS Error THEN
PRINT "error"
ELSE
group.Enter()
group.Leave()
LET gw AS VOID OR Error = group.Wait(100)
LET bw AS BOOLEAN OR Error = barrier.Wait(100)
LET sa AS VOID OR Error = semaphore.Acquire(100)
semaphore.Release()
LET ml AS VOID OR Error = mutex.Lock(100)
mutex.Unlock()
PRINT gw IS Error, bw IS Error, sa IS Error, ml IS Error
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch synchronization primitives");
    assert_eq!(output, "FALSE FALSE FALSE FALSE\n");
}

#[test]
fn bndispatch_wait_executes_named_task_and_completes_ticket() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Work() AS VOID
PRINT "worked"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = queue.Async(Work)
IF ticket IS Error THEN
PRINT "error"
ELSE
LET result AS VOID OR Error = ticket.Wait(100)
PRINT result IS Error, ticket.Status(), ticket.IsDone()
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch task on wait");
    assert_eq!(output, "worked\nFALSE 2 TRUE\n");
}

#[test]
fn bndispatch_async_tasks_use_isolated_worker_contexts() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION First() AS VOID
PRINT "first"
END FUNCTION
FUNCTION Second() AS VOID
PRINT "second"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(2)
IF queue IS Error THEN
PRINT "queue-error"
ELSE
LET one AS Dispatch.Ticket OR Error = queue.Async(First)
LET two AS Dispatch.Ticket OR Error = queue.Async(Second)
IF one IS Error OR two IS Error THEN
PRINT "ticket-error"
ELSE
LET wait_one AS VOID OR Error = one.Wait(1000)
LET wait_two AS VOID OR Error = two.Wait(1000)
LET closed AS VOID OR Error = queue.Close(1000)
PRINT wait_one IS Error, wait_two IS Error, closed IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute isolated BNDispatch workers");
    assert!(
        output.ends_with("FALSE FALSE FALSE\n"),
        "unexpected worker output: {output:?}"
    );
    assert!(output.contains("first\n") && output.contains("second\n"));
}

#[test]
fn typed_async_await_preserves_worker_error() {
    let source = include_str!("../grammar/valid/dispatch-worker-error.bn");
    let (code, output) = run(source, "").expect("worker Error reaches AWAIT caller");
    assert_eq!(code, 0);
    assert_eq!(output, "ERROR\n");
}

#[test]
fn dispatch_game_tournament_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_game_tournament.bn")
        .expect("execute game-tournament dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "CC: A= 3000  B= 3000\nCD: A= 0  B= 5000\nDC: A= 5000  B= 0\nDD: A= 1000  B= 1000\ntournament complete\n"
    );
}

#[test]
fn dispatch_reliability_simulation_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_reliability_simulation.bn")
        .expect("execute reliability dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "line-A: outages= 12  cost= 1200\nline-B: outages= 16  cost= 3200\nline-C: outages= 20  cost= 6000\nline-D: outages= 24  cost= 9600\nreliability simulation complete\n"
    );
}

#[test]
fn dispatch_cellular_automaton_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_cellular_automaton.bn")
        .expect("execute cellular-automaton dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "rule-30 seed-37: checksum= 27\nrule-45 seed-74: checksum= 13\nrule-110 seed-1: checksum= 10\nrule-184 seed-146: checksum= 14\ncellular automaton complete\n"
    );
}
