// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Typed signatures of the `bn_rt` C functions that typed lowering calls.
// A test checks every entry against the `declare` lines the backend emits,
// so a call built from this table matches its declaration.

use crate::ir::{
    AbiType::{Double, I32, I64, Ptr},
    RuntimeFn,
};

pub(crate) const CLOCK_NOW: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_clock_now",
    ret: I64,
    params: [],
};
pub(crate) const CLOCK_TIMER: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_clock_timer",
    ret: I64,
    params: [],
};
pub(crate) const CONSOLE_BEEP: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_console_beep",
    ret: I32,
    params: [],
};
pub(crate) const CONSOLE_CLS: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_console_cls",
    ret: I32,
    params: [],
};
pub(crate) const CONSOLE_NUM_COLS: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_console_num_cols",
    ret: I32,
    params: [],
};
pub(crate) const CONSOLE_NUM_ROWS: RuntimeFn<0> = RuntimeFn {
    name: "bn_rt_console_num_rows",
    ret: I32,
    params: [],
};
pub(crate) const CONSOLE_PRINT_AT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_console_print_at",
    ret: I32,
    params: [I32, I32, Ptr],
};
pub(crate) const DISPATCH_AWAIT: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_dispatch_await",
    ret: I32,
    params: [I64, I64, Ptr, Ptr],
};
pub(crate) const DISPATCH_BARRIER_WAIT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_dispatch_barrier_wait",
    ret: I32,
    params: [I64, I64, Ptr],
};
pub(crate) const DISPATCH_QUEUE_CREATE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_queue_create",
    ret: I32,
    params: [I64, Ptr],
};
pub(crate) const DISPATCH_QUEUE_CREATE_AUTO: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_queue_create_auto",
    ret: I32,
    params: [Ptr],
};
pub(crate) const DISPATCH_TICKET_CANCEL: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_cancel",
    ret: I32,
    params: [I64, Ptr],
};
pub(crate) const DISPATCH_TICKET_ERROR: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_error",
    ret: I32,
    params: [I64, Ptr, Ptr],
};
pub(crate) const DISPATCH_TICKET_ID: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_id",
    ret: I64,
    params: [I64],
};
pub(crate) const DISPATCH_TICKET_IS_DONE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_is_done",
    ret: I32,
    params: [I64],
};
pub(crate) const DISPATCH_TICKET_STATUS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_status",
    ret: I32,
    params: [I64],
};
pub(crate) const ERROR_TAKE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_error_take",
    ret: Ptr,
    params: [I32, Ptr],
};
pub(crate) const ENV_GET: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_env_get",
    ret: I32,
    params: [Ptr, Ptr],
};
pub(crate) const ENV_HAS: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_env_has",
    ret: I32,
    params: [Ptr, Ptr],
};
pub(crate) const EXEC_RESULT_CLOSE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_exec_result_close",
    ret: I32,
    params: [I64],
};
pub(crate) const EXEC_RESULT_RETURN_CODE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_exec_result_return_code",
    ret: I64,
    params: [I64],
};
pub(crate) const EXEC_RUN: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_exec_run",
    ret: I32,
    params: [Ptr, Ptr, I32, Ptr],
};
pub(crate) const HOST_NUM_PROCS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_host_num_procs",
    ret: I32,
    params: [Ptr],
};
pub(crate) const MATH_ACOS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_acos",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_ASIN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_asin",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_ATAN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_atan",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_ATAN2: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_atan2",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_CEIL: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_ceil",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_COS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_cos",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_EXP: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_exp",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_FABS: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_fabs",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_FLOOR: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_floor",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_FMA: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_fma",
    ret: Double,
    params: [Double, Double, Double],
};
pub(crate) const MATH_FMAX: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_fmax",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_FMIN: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_fmin",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_FSIGN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_fsign",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_HYPOT: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_hypot",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_IMAX: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_imax",
    ret: I64,
    params: [I64, I64],
};
pub(crate) const MATH_IMIN: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_imin",
    ret: I64,
    params: [I64, I64],
};
pub(crate) const MATH_ISIGN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_isign",
    ret: I64,
    params: [I64],
};
pub(crate) const MATH_LOG: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_log",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_LOG10: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_log10",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_LOG2: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_log2",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_MEAN_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_mean_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_MEAN_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_mean_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_MEDIAN_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_median_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_MEDIAN_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_median_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_MODE_F64: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_mode_f64",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const MATH_MODE_I32: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_mode_i32",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const MATH_POW: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_pow",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_QUARTILE1_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_quartile1_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_QUARTILE1_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_quartile1_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_QUARTILE3_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_quartile3_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_QUARTILE3_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_quartile3_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_RANGE_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_range_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_RANGE_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_range_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_ROUND: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_round",
    ret: Double,
    params: [Double, Double],
};
pub(crate) const MATH_SIN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_sin",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_SQRT: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_sqrt",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_STDEV_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_stdev_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_STDEV_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_stdev_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_TAN: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_tan",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_TODATE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_todate",
    ret: I32,
    params: [I64, Ptr],
};
pub(crate) const MATH_TOHOUR: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_tohour",
    ret: I32,
    params: [I64],
};
pub(crate) const MATH_TOTIME: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_totime",
    ret: I32,
    params: [I64, Ptr],
};
pub(crate) const MATH_TOTIMESTAMP: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_totimestamp",
    ret: I64,
    params: [I32, I32],
};
pub(crate) const MATH_TOWEEKDAY: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_toweekday",
    ret: I32,
    params: [I64],
};
pub(crate) const MATH_TRUNC: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_trunc",
    ret: Double,
    params: [Double],
};
pub(crate) const MATH_VAL: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_math_val",
    ret: Double,
    params: [Ptr],
};
pub(crate) const MATH_VARIANCE_F64: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_variance_f64",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_VARIANCE_I32: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_math_variance_i32",
    ret: Double,
    params: [Ptr, I32],
};
pub(crate) const MATH_VMAX_F64: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_vmax_f64",
    ret: Double,
    params: [Ptr, I32, Ptr],
};
pub(crate) const MATH_VMAX_I32: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_vmax_i32",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const MATH_VMIN_F64: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_vmin_f64",
    ret: Double,
    params: [Ptr, I32, Ptr],
};
pub(crate) const MATH_VMIN_I32: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_math_vmin_i32",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const NET_ADDRESS_PARSE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_net_address_parse",
    ret: I32,
    params: [Ptr, Ptr],
};
pub(crate) const NET_ADDRESSES_COUNT: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_net_addresses_count",
    ret: I32,
    params: [Ptr],
};
pub(crate) const NET_ADDRESSES_GET: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_addresses_get",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const NET_HANDLE_CLOSE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_net_handle_close",
    ret: I32,
    params: [I64],
};
pub(crate) const NET_NEIGHBOR: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_net_neighbor",
    ret: I32,
    params: [Ptr, Ptr],
};
pub(crate) const NET_PING: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_ping",
    ret: I32,
    params: [Ptr, I32, Ptr, Ptr],
};
pub(crate) const NET_RESOLVE: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_resolve",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const NET_REVERSE: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_reverse",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const NET_TCP_ACCEPT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_tcp_accept",
    ret: I32,
    params: [I64, I32, Ptr],
};
pub(crate) const NET_TCP_CONNECT: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_tcp_connect",
    ret: I32,
    params: [Ptr, I32, I32, Ptr],
};
pub(crate) const NET_TCP_LISTEN_WITH_BACKLOG: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_tcp_listen_with_backlog",
    ret: I32,
    params: [Ptr, I32, I32, Ptr],
};
pub(crate) const NET_TCP_READ: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_tcp_read",
    ret: I32,
    params: [I64, Ptr, I32, Ptr],
};
pub(crate) const NET_TCP_WRITE: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_tcp_write",
    ret: I32,
    params: [I64, Ptr, I32, Ptr],
};
pub(crate) const NET_UDP_BIND: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_udp_bind",
    ret: I32,
    params: [Ptr, I32, Ptr],
};
pub(crate) const NET_UDP_PACKET_COPY_TO: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_udp_packet_copy_to",
    ret: I32,
    params: [I64, Ptr, I32, Ptr],
};
pub(crate) const NET_UDP_PACKET_SOURCE: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_udp_packet_source",
    ret: I32,
    params: [I64, Ptr, Ptr],
};
pub(crate) const NET_UDP_RECEIVE_HANDLE: RuntimeFn<4> = RuntimeFn {
    name: "bn_rt_net_udp_receive_handle",
    ret: I32,
    params: [I64, I32, I32, Ptr],
};
pub(crate) const NET_UDP_SEND_TO: RuntimeFn<6> = RuntimeFn {
    name: "bn_rt_net_udp_send_to",
    ret: I32,
    params: [I64, Ptr, I32, Ptr, I32, Ptr],
};

pub(crate) const EXEC_RESULT_STDOUT: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_exec_result_stdout",
    ret: Ptr,
    params: [I64],
};

pub(crate) const EXEC_RESULT_STDERR: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_exec_result_stderr",
    ret: Ptr,
    params: [I64],
};

pub(crate) const DISPATCH_GROUP_CREATE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_group_create",
    ret: I32,
    params: [Ptr],
};

pub(crate) const DISPATCH_BARRIER_CREATE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_barrier_create",
    ret: I32,
    params: [I64, Ptr],
};

pub(crate) const DISPATCH_SEMAPHORE_CREATE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_semaphore_create",
    ret: I32,
    params: [I64, Ptr],
};

pub(crate) const DISPATCH_MUTEX_CREATE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_mutex_create",
    ret: I32,
    params: [Ptr],
};

pub(crate) const DISPATCH_QUEUE_JOIN: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_queue_join",
    ret: I32,
    params: [I64, I64],
};

pub(crate) const DISPATCH_QUEUE_CLOSE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_queue_close",
    ret: I32,
    params: [I64, I64],
};

pub(crate) const DISPATCH_GROUP_WAIT: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_group_wait",
    ret: I32,
    params: [I64, I64],
};

pub(crate) const DISPATCH_GROUP_ENTER: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_group_enter",
    ret: I32,
    params: [I64],
};

pub(crate) const DISPATCH_GROUP_LEAVE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_group_leave",
    ret: I32,
    params: [I64],
};

pub(crate) const DISPATCH_SEMAPHORE_ACQUIRE: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_semaphore_acquire",
    ret: I32,
    params: [I64, I64],
};

pub(crate) const DISPATCH_SEMAPHORE_RELEASE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_semaphore_release",
    ret: I32,
    params: [I64],
};

pub(crate) const DISPATCH_MUTEX_LOCK: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_dispatch_mutex_lock",
    ret: I32,
    params: [I64, I64],
};

pub(crate) const DISPATCH_MUTEX_UNLOCK: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_mutex_unlock",
    ret: I32,
    params: [I64],
};

pub(crate) const DISPATCH_TICKET_CLOSE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_dispatch_ticket_close",
    ret: I32,
    params: [I64],
};

pub(crate) const NET_TCP_STREAM_LOCAL_ENDPOINT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_tcp_stream_local_endpoint",
    ret: I32,
    params: [I64, Ptr, Ptr],
};

pub(crate) const NET_TCP_STREAM_REMOTE_ENDPOINT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_tcp_stream_remote_endpoint",
    ret: I32,
    params: [I64, Ptr, Ptr],
};

pub(crate) const NET_UDP_PACKET_SIZE: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_net_udp_packet_size",
    ret: I32,
    params: [I64],
};

pub(crate) const NET_UDP_PACKET_TRUNCATED: RuntimeFn<1> = RuntimeFn {
    name: "bn_rt_net_udp_packet_truncated",
    ret: I32,
    params: [I64],
};

pub(crate) const NET_UDP_LOCAL_ENDPOINT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_udp_local_endpoint",
    ret: I32,
    params: [I64, Ptr, Ptr],
};

pub(crate) const NET_TCP_LISTENER_LOCAL_ENDPOINT: RuntimeFn<3> = RuntimeFn {
    name: "bn_rt_net_tcp_listener_local_endpoint",
    ret: I32,
    params: [I64, Ptr, Ptr],
};

pub(crate) const STR_EQ: RuntimeFn<2> = RuntimeFn {
    name: "bn_rt_str_eq",
    ret: I32,
    params: [Ptr, Ptr],
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Declaration;

    const ALL: &[&dyn Declaration] = &[
        &STR_EQ,
        &NET_TCP_LISTENER_LOCAL_ENDPOINT,
        &NET_UDP_LOCAL_ENDPOINT,
        &NET_UDP_PACKET_TRUNCATED,
        &NET_UDP_PACKET_SIZE,
        &NET_TCP_STREAM_REMOTE_ENDPOINT,
        &NET_TCP_STREAM_LOCAL_ENDPOINT,
        &DISPATCH_TICKET_CLOSE,
        &DISPATCH_MUTEX_UNLOCK,
        &DISPATCH_MUTEX_LOCK,
        &DISPATCH_SEMAPHORE_RELEASE,
        &DISPATCH_SEMAPHORE_ACQUIRE,
        &DISPATCH_GROUP_LEAVE,
        &DISPATCH_GROUP_ENTER,
        &DISPATCH_GROUP_WAIT,
        &DISPATCH_QUEUE_CLOSE,
        &DISPATCH_QUEUE_JOIN,
        &DISPATCH_MUTEX_CREATE,
        &DISPATCH_SEMAPHORE_CREATE,
        &DISPATCH_BARRIER_CREATE,
        &DISPATCH_GROUP_CREATE,
        &EXEC_RESULT_STDERR,
        &EXEC_RESULT_STDOUT,
        &CLOCK_NOW,
        &CLOCK_TIMER,
        &CONSOLE_BEEP,
        &CONSOLE_CLS,
        &CONSOLE_NUM_COLS,
        &CONSOLE_NUM_ROWS,
        &CONSOLE_PRINT_AT,
        &DISPATCH_AWAIT,
        &DISPATCH_BARRIER_WAIT,
        &DISPATCH_QUEUE_CREATE,
        &DISPATCH_QUEUE_CREATE_AUTO,
        &DISPATCH_TICKET_CANCEL,
        &DISPATCH_TICKET_ERROR,
        &DISPATCH_TICKET_ID,
        &DISPATCH_TICKET_IS_DONE,
        &DISPATCH_TICKET_STATUS,
        &ERROR_TAKE,
        &EXEC_RESULT_CLOSE,
        &EXEC_RESULT_RETURN_CODE,
        &EXEC_RUN,
        &HOST_NUM_PROCS,
        &MATH_ACOS,
        &MATH_ASIN,
        &MATH_ATAN,
        &MATH_ATAN2,
        &MATH_CEIL,
        &MATH_COS,
        &MATH_EXP,
        &MATH_FABS,
        &MATH_FLOOR,
        &MATH_FMA,
        &MATH_FMAX,
        &MATH_FMIN,
        &MATH_FSIGN,
        &MATH_HYPOT,
        &MATH_IMAX,
        &MATH_IMIN,
        &MATH_ISIGN,
        &MATH_LOG,
        &MATH_LOG10,
        &MATH_LOG2,
        &MATH_MEAN_F64,
        &MATH_MEAN_I32,
        &MATH_MEDIAN_F64,
        &MATH_MEDIAN_I32,
        &MATH_MODE_F64,
        &MATH_MODE_I32,
        &MATH_POW,
        &MATH_QUARTILE1_F64,
        &MATH_QUARTILE1_I32,
        &MATH_QUARTILE3_F64,
        &MATH_QUARTILE3_I32,
        &MATH_RANGE_F64,
        &MATH_RANGE_I32,
        &MATH_ROUND,
        &MATH_SIN,
        &MATH_SQRT,
        &MATH_STDEV_F64,
        &MATH_STDEV_I32,
        &MATH_TAN,
        &MATH_TODATE,
        &MATH_TOHOUR,
        &MATH_TOTIME,
        &MATH_TOTIMESTAMP,
        &MATH_TOWEEKDAY,
        &MATH_TRUNC,
        &MATH_VAL,
        &MATH_VARIANCE_F64,
        &MATH_VARIANCE_I32,
        &MATH_VMAX_F64,
        &MATH_VMAX_I32,
        &MATH_VMIN_F64,
        &MATH_VMIN_I32,
        &NET_ADDRESS_PARSE,
        &NET_ADDRESSES_COUNT,
        &NET_ADDRESSES_GET,
        &NET_HANDLE_CLOSE,
        &NET_NEIGHBOR,
        &NET_PING,
        &NET_RESOLVE,
        &NET_REVERSE,
        &NET_TCP_ACCEPT,
        &NET_TCP_CONNECT,
        &NET_TCP_LISTEN_WITH_BACKLOG,
        &NET_TCP_READ,
        &NET_TCP_WRITE,
        &NET_UDP_BIND,
        &NET_UDP_PACKET_COPY_TO,
        &NET_UDP_PACKET_SOURCE,
        &NET_UDP_RECEIVE_HANDLE,
        &NET_UDP_SEND_TO,
    ];

    /// Every literal `declare` the backend sources contain, wherever it sits
    /// (a `const` table, a one-off string, a multi-line literal).
    fn emitted_declarations() -> String {
        let sources = [
            include_str!("runtime.rs"),
            include_str!("math.rs"),
            include_str!("power_shift.rs"),
            include_str!("preamble.rs"),
            include_str!("host_results.rs"),
            include_str!("helpers.rs"),
            include_str!("platform_stdio.rs"),
            include_str!("traps.rs"),
        ];
        let mut lines = String::new();
        for source in sources {
            for (index, _) in source.match_indices("declare ") {
                let rest = &source[index..];
                if let Some(end) = rest.find(')') {
                    lines.push_str(&rest[..=end]);
                    lines.push('\n');
                }
            }
        }
        lines
    }

    #[test]
    fn every_typed_signature_matches_an_emitted_declaration() {
        let emitted = emitted_declarations();
        let lines = emitted.lines().collect::<std::collections::HashSet<_>>();
        for function in ALL {
            let declaration = function.declaration();
            assert!(
                lines.contains(declaration.as_str()),
                "no emitted declaration matches `{declaration}`"
            );
        }
    }
}
