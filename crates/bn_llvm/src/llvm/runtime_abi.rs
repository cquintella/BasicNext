// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Every C function the backend declares (`bn_rt`, the C library and the
// LLVM intrinsics with C types) as one typed signature, and the groups a
// module declares together, in emission order. No `declare` is written as
// text anywhere else (bucket 0.6.5 T.5.1).

use crate::ir::{
    AbiType::{Double, Float, I1, I8, I16, I32, I64, Ptr, Void},
    Declaration, Intrinsic, LlvmType, RuntimeFn, VariadicFn,
};

/// One `RuntimeFn` constant per line: `NAME = "symbol"(parameters) -> result;`.
macro_rules! signatures {
    ($($konst:ident = $name:literal ($($param:ident),*) -> $ret:ident;)*) => {
        $(pub(crate) const $konst: RuntimeFn<{ <[&str]>::len(&[$(stringify!($param)),*]) }> =
            RuntimeFn { name: $name, ret: $ret, params: [$($param),*] };)*
    };
}

/// One declaration group: the signatures a module declares together, in order.
macro_rules! declarations {
    ($($group:ident = $($konst:ident)*;)*) => {
        $(pub(crate) const $group: &[&dyn Declaration] = &[$(&$konst),*];)*
    };
}

signatures! {
    ARC_ADDRESS = "bn_rt_arc_address"(I64) -> Ptr;
    ARC_FINISH_DESTROY = "bn_rt_arc_finish_destroy"(I64) -> I32;
    ARC_REGISTER = "bn_rt_arc_register"(Ptr, Ptr, I32, I32) -> I64;
    ARC_RELEASE = "bn_rt_arc_release"(I64, I32, I32) -> I32;
    ARC_RETAIN = "bn_rt_arc_retain"(I64, I32, I32) -> I32;
    ACRT_IOB_FUNC = "__acrt_iob_func"(I32) -> Ptr;
    CALLOC = "calloc"(I64, I64) -> Ptr;
    CLOCK_NOW = "bn_rt_clock_now"() -> I64;
    CLOCK_TIMER = "bn_rt_clock_timer"() -> I64;
    CONSOLE_BEEP = "bn_rt_console_beep"() -> I32;
    CONSOLE_CLS = "bn_rt_console_cls"() -> I32;
    CONSOLE_NUM_COLS = "bn_rt_console_num_cols"() -> I32;
    CONSOLE_NUM_ROWS = "bn_rt_console_num_rows"() -> I32;
    CONSOLE_PRINT_AT = "bn_rt_console_print_at"(I32, I32, Ptr) -> I32;
    COMPRESS_DEFLATE = "bn_rt_compress_deflate"(I64, Ptr) -> I32;
    COMPRESS_INFLATE = "bn_rt_compress_inflate"(I64, Ptr) -> I32;
    COMPRESS_GZIP = "bn_rt_compress_gzip"(I64, Ptr) -> I32;
    COMPRESS_GUNZIP = "bn_rt_compress_gunzip"(I64, Ptr) -> I32;
    COMPRESS_DEFLATE_TEXT = "bn_rt_compress_deflate_text"(Ptr, Ptr) -> I32;
    COMPRESS_INFLATE_TEXT = "bn_rt_compress_inflate_text"(I64, Ptr) -> I32;
    COMPRESS_GZIP_TEXT = "bn_rt_compress_gzip_text"(Ptr, Ptr) -> I32;
    COMPRESS_GUNZIP_TEXT = "bn_rt_compress_gunzip_text"(I64, Ptr) -> I32;
    COMPRESS_BYTES_FROM_HEX = "bn_rt_compress_bytes_from_hex"(Ptr, Ptr) -> I32;
    COMPRESS_BYTES_FROM_TEXT = "bn_rt_compress_bytes_from_text"(Ptr) -> I64;
    COMPRESS_BYTES_LENGTH = "bn_rt_compress_bytes_length"(I64) -> I64;
    COMPRESS_BYTES_RELEASE = "bn_rt_compress_bytes_release"(I64) -> I32;
    COMPRESS_BYTES_RETAIN = "bn_rt_compress_bytes_retain"(I64) -> I32;
    COMPRESS_BYTES_TO_HEX = "bn_rt_compress_bytes_to_hex"(I64) -> Ptr;
    COMPRESS_BYTES_TO_TEXT = "bn_rt_compress_bytes_to_text"(I64, Ptr) -> I32;
    ZIP_CREATE = "bn_rt_zip_create"(Ptr, Ptr) -> I32;
    ZIP_OPEN = "bn_rt_zip_open"(Ptr, Ptr) -> I32;
    ZIP_ADD_FILE = "bn_rt_zip_add_file"(I64, Ptr, Ptr) -> I32;
    ZIP_ADD_TEXT = "bn_rt_zip_add_text"(I64, Ptr, Ptr) -> I32;
    ZIP_ADD_STORED_TEXT = "bn_rt_zip_add_stored_text"(I64, Ptr, Ptr) -> I32;
    ZIP_ADD_BYTES = "bn_rt_zip_add_bytes"(I64, Ptr, I64) -> I32;
    ZIP_ADD_STORED_BYTES = "bn_rt_zip_add_stored_bytes"(I64, Ptr, I64) -> I32;
    ZIP_EXTRACT_ALL = "bn_rt_zip_extract_all"(I64, Ptr) -> I32;
    ZIP_EXTRACT_FILE = "bn_rt_zip_extract_file"(I64, Ptr, Ptr) -> I32;
    ZIP_EXTRACT_TEXT = "bn_rt_zip_extract_text"(I64, Ptr, Ptr) -> I32;
    ZIP_EXTRACT_BYTES = "bn_rt_zip_extract_bytes"(I64, Ptr, Ptr) -> I32;
    ZIP_ENTRY_COUNT = "bn_rt_zip_entry_count"(I64) -> I64;
    ZIP_ENTRY_NAME = "bn_rt_zip_entry_name"(I64, I64, Ptr) -> I32;
    ZIP_CLOSE = "bn_rt_zip_close"(I64) -> I32;
    ZIP_RETAIN = "bn_rt_zip_retain"(I64) -> I32;
    ZIP_RELEASE = "bn_rt_zip_release"(I64) -> I32;
    CRYPTO_ARGON2ID = "bn_rt_crypto_argon2id"(I64, I64, I64, I64, I64, Ptr) -> I32;
    CRYPTO_BYTES_FROM_HEX = "bn_rt_crypto_bytes_from_hex"(Ptr, Ptr) -> I32;
    CRYPTO_BYTES_FROM_TEXT = "bn_rt_crypto_bytes_from_text"(Ptr) -> I64;
    CRYPTO_BYTES_LENGTH = "bn_rt_crypto_bytes_length"(I64) -> I64;
    CRYPTO_BYTES_RELEASE = "bn_rt_crypto_bytes_release"(I64) -> I32;
    CRYPTO_BYTES_RETAIN = "bn_rt_crypto_bytes_retain"(I64) -> I32;
    CRYPTO_BYTES_TO_HEX = "bn_rt_crypto_bytes_to_hex"(I64) -> Ptr;
    CRYPTO_DSA_KEYPAIR = "bn_rt_crypto_dsa_keypair"(I64, Ptr) -> I32;
    CRYPTO_DSA_SIGN = "bn_rt_crypto_dsa_sign"(I64, I64, Ptr) -> I32;
    CRYPTO_DSA_VERIFY = "bn_rt_crypto_dsa_verify"(I64, I64, I64) -> I32;
    CRYPTO_GENERATE_KEY = "bn_rt_crypto_generate_key"(I32, Ptr) -> I32;
    CRYPTO_GENERATE_NONCE = "bn_rt_crypto_generate_nonce"(I32, Ptr) -> I32;
    CRYPTO_HMAC = "bn_rt_crypto_hmac"(I64, I64, Ptr) -> I32;
    CRYPTO_HMAC_VERIFY = "bn_rt_crypto_hmac_verify"(I64, I64, I64) -> I32;
    CRYPTO_IMPORT_KEY = "bn_rt_crypto_import_key"(I64, I32, Ptr) -> I32;
    CRYPTO_KEM_DECAPSULATE = "bn_rt_crypto_kem_decapsulate"(I64, I64, Ptr) -> I32;
    CRYPTO_KEM_ENCAPSULATE = "bn_rt_crypto_kem_encapsulate"(I64, Ptr) -> I32;
    CRYPTO_KEM_KEYPAIR = "bn_rt_crypto_kem_keypair"(I64, Ptr) -> I32;
    CRYPTO_KEY_ALGORITHM = "bn_rt_crypto_key_algorithm"(I64) -> I32;
    CRYPTO_KEY_LENGTH = "bn_rt_crypto_key_length"(I64) -> I64;
    CRYPTO_KEY_RELEASE = "bn_rt_crypto_key_release"(I64) -> I32;
    CRYPTO_KEY_RETAIN = "bn_rt_crypto_key_retain"(I64) -> I32;
    CRYPTO_KEY_ZEROIZE = "bn_rt_crypto_key_zeroize"(I64) -> I32;
    CRYPTO_OPEN = "bn_rt_crypto_open"(I32, I64, I64, I64, I64, Ptr) -> I32;
    CRYPTO_PUBLIC_KEY = "bn_rt_crypto_public_key"(I32, I64, Ptr) -> I32;
    CRYPTO_RANDOM_BYTES = "bn_rt_crypto_random_bytes"(I64, Ptr) -> I32;
    CRYPTO_SEAL = "bn_rt_crypto_seal"(I32, I64, I64, I64, I64, Ptr) -> I32;
    CRYPTO_SHA256 = "bn_rt_crypto_sha256"(Ptr) -> Ptr;
    CRYPTO_SHA512 = "bn_rt_crypto_sha512"(Ptr) -> Ptr;
    CRYPTO_SIGN = "bn_rt_crypto_sign"(I32, I64, I64, Ptr) -> I32;
    CRYPTO_SLICE = "bn_rt_crypto_slice"(I64, I64, I64, Ptr) -> I32;
    CRYPTO_VERIFY = "bn_rt_crypto_verify"(I32, I64, I64, I64) -> I32;
    DATAFRAME_ADD_BOOLEAN = "bn_rt_dataframe_add_boolean"(I64, Ptr, Ptr, I32) -> I32;
    DATAFRAME_ADD_FLOAT = "bn_rt_dataframe_add_float"(I64, Ptr, Ptr, I32) -> I32;
    DATAFRAME_ADD_INTEGER_START = "bn_rt_dataframe_add_integer_start"(I64, Ptr, I32) -> I32;
    DATAFRAME_ADD_STRING = "bn_rt_dataframe_add_string"(I64, Ptr, Ptr, I32) -> I32;
    DATAFRAME_APPEND_COLUMNS = "bn_rt_dataframe_append_columns"(I64, I64, Ptr) -> I32;
    DATAFRAME_APPEND_ROWS = "bn_rt_dataframe_append_rows"(I64, I64, Ptr) -> I32;
    DATAFRAME_CLOSE = "bn_rt_dataframe_close"(I64) -> I32;
    DATAFRAME_COLUMN_COUNT = "bn_rt_dataframe_column_count"(I64, Ptr) -> I32;
    DATAFRAME_COLUMN_NAME_OWNED = "bn_rt_dataframe_column_name_owned"(I64, I32) -> Ptr;
    DATAFRAME_CONVERT_FLOAT = "bn_rt_dataframe_convert_float"(I64, Ptr) -> I32;
    DATAFRAME_COUNT_NA = "bn_rt_dataframe_count_na"(I64, Ptr, Ptr) -> I32;
    DATAFRAME_DROP_NA = "bn_rt_dataframe_drop_na"(I64, Ptr) -> I32;
    DATAFRAME_FILL_NA = "bn_rt_dataframe_fill_na"(I64, Ptr, Double) -> I32;
    DATAFRAME_CONVERT_INTEGER = "bn_rt_dataframe_convert_integer"(I64, Ptr) -> I32;
    DATAFRAME_COPY_FLOAT = "bn_rt_dataframe_copy_float"(I64, Ptr, Ptr, I32) -> I32;
    DATAFRAME_COPY_INTEGER = "bn_rt_dataframe_copy_integer"(I64, Ptr, Ptr, I32) -> I32;
    DATAFRAME_CREATE = "bn_rt_dataframe_create"(Ptr, I32, Ptr) -> I32;
    DATAFRAME_GET_BOOLEAN = "bn_rt_dataframe_get_boolean"(I64, I32, Ptr, Ptr, Ptr) -> I32;
    DATAFRAME_GET_FLOAT = "bn_rt_dataframe_get_float"(I64, I32, Ptr, Ptr, Ptr) -> I32;
    DATAFRAME_GET_INTEGER = "bn_rt_dataframe_get_integer"(I64, I32, Ptr, Ptr, Ptr) -> I32;
    DATAFRAME_GET_STRING = "bn_rt_dataframe_get_string"(I64, I32, Ptr, Ptr, Ptr) -> I32;
    DATAFRAME_JOIN = "bn_rt_dataframe_join"(I64, I64, Ptr, Ptr, I32, Ptr) -> I32;
    DATAFRAME_READ_CSV = "bn_rt_dataframe_read_csv"(I64, I8, Ptr, Ptr) -> I32;
    DATAFRAME_REDUCE = "bn_rt_dataframe_reduce"(I64, Ptr, I32, Ptr, Ptr) -> I32;
    DATAFRAME_RETAIN = "bn_rt_dataframe_retain"(I64) -> I32;
    DATAFRAME_ROW_COUNT = "bn_rt_dataframe_row_count"(I64, Ptr) -> I32;
    DATAFRAME_SELECT = "bn_rt_dataframe_select"(I64, Ptr, I32, Ptr, I32, Ptr) -> I32;
    DATAFRAME_SET_INTEGER_CELL = "bn_rt_dataframe_set_integer_cell"(I64, I32, I32, I64) -> I32;
    DATAFRAME_SET_LABEL = "bn_rt_dataframe_set_label"(I64, Ptr, Ptr) -> I32;
    DATAFRAME_SLICE = "bn_rt_dataframe_slice"(I64, I32, I32, I32, I32, Ptr) -> I32;
    DATAFRAME_TRANSPOSE = "bn_rt_dataframe_transpose"(I64, Ptr) -> I32;
    DATAFRAME_WRITE_CSV = "bn_rt_dataframe_write_csv"(I64, I64, I8, Ptr) -> I32;
    DATAFRAME_ZSCORE = "bn_rt_dataframe_zscore"(I64, Ptr, Ptr) -> I32;
    DISPATCH_AWAIT = "bn_rt_dispatch_await"(I64, I64, Ptr, Ptr) -> I32;
    DISPATCH_BARRIER_CLOSE = "bn_rt_dispatch_barrier_close"(I64) -> I32;
    DISPATCH_BARRIER_CREATE = "bn_rt_dispatch_barrier_create"(I64, Ptr) -> I32;
    DISPATCH_BARRIER_WAIT = "bn_rt_dispatch_barrier_wait"(I64, I64, Ptr) -> I32;
    DISPATCH_CANCEL = "bn_rt_dispatch_cancel"(I64) -> I32;
    DISPATCH_GROUP_CLOSE = "bn_rt_dispatch_group_close"(I64) -> I32;
    DISPATCH_GROUP_CREATE = "bn_rt_dispatch_group_create"(Ptr) -> I32;
    DISPATCH_GROUP_ENTER = "bn_rt_dispatch_group_enter"(I64) -> I32;
    DISPATCH_GROUP_LEAVE = "bn_rt_dispatch_group_leave"(I64) -> I32;
    DISPATCH_GROUP_WAIT = "bn_rt_dispatch_group_wait"(I64, I64) -> I32;
    DISPATCH_MUTEX_CLOSE = "bn_rt_dispatch_mutex_close"(I64) -> I32;
    DISPATCH_MUTEX_CREATE = "bn_rt_dispatch_mutex_create"(Ptr) -> I32;
    DISPATCH_MUTEX_LOCK = "bn_rt_dispatch_mutex_lock"(I64, I64) -> I32;
    DISPATCH_MUTEX_UNLOCK = "bn_rt_dispatch_mutex_unlock"(I64) -> I32;
    DISPATCH_QUEUE_CLOSE = "bn_rt_dispatch_queue_close"(I64, I64) -> I32;
    DISPATCH_QUEUE_CREATE = "bn_rt_dispatch_queue_create"(I64, Ptr) -> I32;
    DISPATCH_QUEUE_CREATE_AUTO = "bn_rt_dispatch_queue_create_auto"(Ptr) -> I32;
    DISPATCH_QUEUE_JOIN = "bn_rt_dispatch_queue_join"(I64, I64) -> I32;
    DISPATCH_SEMAPHORE_ACQUIRE = "bn_rt_dispatch_semaphore_acquire"(I64, I64) -> I32;
    DISPATCH_SEMAPHORE_CLOSE = "bn_rt_dispatch_semaphore_close"(I64) -> I32;
    DISPATCH_SEMAPHORE_CREATE = "bn_rt_dispatch_semaphore_create"(I64, Ptr) -> I32;
    DISPATCH_SEMAPHORE_RELEASE = "bn_rt_dispatch_semaphore_release"(I64) -> I32;
    DISPATCH_SUBMIT = "bn_rt_dispatch_submit"(I64, Ptr, Ptr, Ptr, I32, Ptr) -> I32;
    DISPATCH_TICKET_CANCEL = "bn_rt_dispatch_ticket_cancel"(I64, Ptr) -> I32;
    DISPATCH_TICKET_CLOSE = "bn_rt_dispatch_ticket_close"(I64) -> I32;
    DISPATCH_TICKET_ERROR = "bn_rt_dispatch_ticket_error"(I64, Ptr, Ptr) -> I32;
    DISPATCH_TICKET_ID = "bn_rt_dispatch_ticket_id"(I64) -> I64;
    DISPATCH_TICKET_IS_DONE = "bn_rt_dispatch_ticket_is_done"(I64) -> I32;
    DISPATCH_TICKET_STATUS = "bn_rt_dispatch_ticket_status"(I64) -> I32;
    ENV_GET = "bn_rt_env_get"(Ptr, Ptr) -> I32;
    ENV_HAS = "bn_rt_env_has"(Ptr, Ptr) -> I32;
    ERROR_CODE = "bn_rt_error_code"(Ptr) -> I64;
    ERROR_FIELD = "bn_rt_error_field"(Ptr, I32) -> Ptr;
    ERROR_PRINT = "bn_rt_error_print"(I64, Ptr) -> Void;
    ERROR_SET_REPORT = "bn_rt_error_set_report"(I32, Ptr, Ptr, Ptr) -> Void;
    ERROR_TAKE = "bn_rt_error_take"(I32, Ptr) -> Ptr;
    ERROR_WRAP = "bn_rt_error_wrap"(I1, Ptr, Ptr) -> Ptr;
    EXEC_RESULT_CLOSE = "bn_rt_exec_result_close"(I64) -> I32;
    EXEC_RESULT_RETURN_CODE = "bn_rt_exec_result_return_code"(I64) -> I64;
    EXEC_RESULT_STDERR = "bn_rt_exec_result_stderr"(I64) -> Ptr;
    EXEC_RESULT_STDOUT = "bn_rt_exec_result_stdout"(I64) -> Ptr;
    EXEC_RUN = "bn_rt_exec_run"(Ptr, Ptr, I32, Ptr) -> I32;
    EXIT = "exit"(I32) -> Void;
    FILE_CLOSE = "bn_rt_file_close"(I64) -> I32;
    FILE_NEW = "bn_rt_file_new"(Ptr) -> I32;
    FILE_OPEN = "bn_rt_file_open"(Ptr, I32, Ptr) -> I32;
    FILE_READ_ALL = "bn_rt_file_read_all"(I64, Ptr) -> I32;
    FILE_READ_BYTES = "bn_rt_file_read_bytes"(I64, Ptr, I64, Ptr) -> I32;
    FILE_READ_LINE = "bn_rt_file_read_line"(I64, Ptr) -> I32;
    FILE_RELEASE = "bn_rt_file_release"(I64) -> I32;
    FILE_RETAIN = "bn_rt_file_retain"(I64) -> I32;
    FILE_WRITE = "bn_rt_file_write"(I64, Ptr) -> I32;
    FILE_WRITE_BYTES = "bn_rt_file_write_bytes"(I64, Ptr, I64) -> I32;
    FILE_WRITE_LINE = "bn_rt_file_write_line"(I64, Ptr) -> I32;
    FLOCKFILE = "flockfile"(Ptr) -> Void;
    FREE = "free"(Ptr) -> Void;
    FS_DELETE_FILE = "bn_rt_fs_delete_file"(Ptr) -> I32;
    FS_EXISTS = "bn_rt_fs_exists"(Ptr, Ptr) -> I32;
    FUNLOCKFILE = "funlockfile"(Ptr) -> Void;
    HOST_NUM_PROCS = "bn_rt_host_num_procs"(Ptr) -> I32;
    JSON_APPEND_BOOLEAN = "bn_rt_json_append_boolean"(I64, I32) -> I32;
    JSON_APPEND_FLOAT = "bn_rt_json_append_float"(I64, Double) -> I32;
    JSON_APPEND_INTEGER = "bn_rt_json_append_integer"(I64, I64) -> I32;
    JSON_APPEND_JSON = "bn_rt_json_append_json"(I64, I64) -> I32;
    JSON_APPEND_NULL = "bn_rt_json_append_null"(I64) -> I32;
    JSON_APPEND_STRING = "bn_rt_json_append_string"(I64, Ptr) -> I32;
    JSON_ARRAY = "bn_rt_json_array"() -> I64;
    JSON_CLONE = "bn_rt_json_clone"(I64, Ptr) -> I32;
    JSON_GET_BOOLEAN = "bn_rt_json_get_boolean"(I64, Ptr, Ptr) -> I32;
    JSON_GET_BOOLEAN_AT = "bn_rt_json_get_boolean_at"(I64, I64, Ptr) -> I32;
    JSON_GET_FLOAT = "bn_rt_json_get_float"(I64, Ptr, Ptr) -> Double;
    JSON_GET_FLOAT_AT = "bn_rt_json_get_float_at"(I64, I64, Ptr) -> Double;
    JSON_GET_INTEGER = "bn_rt_json_get_integer"(I64, Ptr, Ptr) -> I64;
    JSON_GET_INTEGER_AT = "bn_rt_json_get_integer_at"(I64, I64, Ptr) -> I64;
    JSON_GET_JSON = "bn_rt_json_get_json"(I64, Ptr, Ptr) -> I32;
    JSON_GET_JSON_AT = "bn_rt_json_get_json_at"(I64, I64, Ptr) -> I32;
    JSON_GET_STRING = "bn_rt_json_get_string"(I64, Ptr, Ptr) -> Ptr;
    JSON_GET_STRING_AT = "bn_rt_json_get_string_at"(I64, I64, Ptr) -> Ptr;
    JSON_HAS = "bn_rt_json_has"(I64, Ptr) -> I32;
    JSON_KIND = "bn_rt_json_kind"(I64) -> Ptr;
    JSON_LENGTH = "bn_rt_json_length"(I64) -> I64;
    JSON_OBJECT = "bn_rt_json_object"() -> I64;
    JSON_PARSE = "bn_rt_json_parse"(Ptr, Ptr) -> I32;
    JSON_RELEASE = "bn_rt_json_release"(I64) -> I32;
    JSON_RETAIN = "bn_rt_json_retain"(I64) -> I32;
    JSON_SET_BOOLEAN = "bn_rt_json_set_boolean"(I64, Ptr, I32) -> I32;
    JSON_SET_BOOLEAN_AT = "bn_rt_json_set_boolean_at"(I64, I64, I32) -> I32;
    JSON_SET_FLOAT = "bn_rt_json_set_float"(I64, Ptr, Double) -> I32;
    JSON_SET_FLOAT_AT = "bn_rt_json_set_float_at"(I64, I64, Double) -> I32;
    JSON_SET_INTEGER = "bn_rt_json_set_integer"(I64, Ptr, I64) -> I32;
    JSON_SET_INTEGER_AT = "bn_rt_json_set_integer_at"(I64, I64, I64) -> I32;
    JSON_SET_JSON = "bn_rt_json_set_json"(I64, Ptr, I64) -> I32;
    JSON_SET_JSON_AT = "bn_rt_json_set_json_at"(I64, I64, I64) -> I32;
    JSON_SET_NULL = "bn_rt_json_set_null"(I64, Ptr) -> I32;
    JSON_SET_NULL_AT = "bn_rt_json_set_null_at"(I64, I64) -> I32;
    JSON_SET_STRING = "bn_rt_json_set_string"(I64, Ptr, Ptr) -> I32;
    JSON_SET_STRING_AT = "bn_rt_json_set_string_at"(I64, I64, Ptr) -> I32;
    JSON_STRINGIFY = "bn_rt_json_stringify"(I64, Ptr) -> Ptr;
    LLVM_MEMCPY_P0_P0_I64 = "llvm.memcpy.p0.p0.i64"(Ptr, Ptr, I64, I1) -> Void;
    LOCK_FILE = "_lock_file"(Ptr) -> Void;
    LOG_CLIENT_ACCESS = "bn_rt_log_client_access"(I64, Ptr, Ptr, I64, I64, I64, Ptr) -> I32;
    LOG_LOGGER_ACCESS = "bn_rt_log_logger_access"(I64, Ptr, Ptr, Ptr, Ptr, I64, I64, Ptr, Ptr) -> I32;
    LOG_LOGGER_CLOSE = "bn_rt_log_logger_close"(I64, I64) -> I32;
    LOG_LOGGER_DELETE = "bn_rt_log_logger_delete"(I64) -> I32;
    LOG_LOGGER_FLUSH = "bn_rt_log_logger_flush"(I64, I64) -> I32;
    LOG_LOGGER_NEW = "bn_rt_log_logger_new"(Ptr, Ptr, Ptr, Ptr) -> I32;
    LOG_LOGGER_RETAIN = "bn_rt_log_logger_retain"(I64) -> I32;
    LOG_LOGGER_SET_ACCESS_DEFAULTS = "bn_rt_log_logger_set_access_defaults"(I64, Ptr, Ptr, Ptr, Ptr, I64, I64, Ptr, Ptr) -> I32;
    LOG_LOGGER_SET_DEFAULTS = "bn_rt_log_logger_set_defaults"(I64, Ptr, Ptr, Ptr, Ptr, Ptr, I64) -> I32;
    LOG_LOGGER_SET_LEVEL = "bn_rt_log_logger_set_level"(I64, I64) -> I32;
    LOG_LOGGER_WRITE = "bn_rt_log_logger_write"(I64, I64, Ptr, Ptr, Ptr, Ptr, Ptr, Ptr, I64) -> I32;
    MALLOC = "malloc"(I64) -> Ptr;
    MATH_ACOS = "bn_rt_math_acos"(Double) -> Double;
    MATH_ASIN = "bn_rt_math_asin"(Double) -> Double;
    MATH_ATAN = "bn_rt_math_atan"(Double) -> Double;
    MATH_ATAN2 = "bn_rt_math_atan2"(Double, Double) -> Double;
    MATH_CEIL = "bn_rt_math_ceil"(Double) -> Double;
    MATH_COS = "bn_rt_math_cos"(Double) -> Double;
    MATH_DEGREE_TO_RAD = "bn_rt_math_degree_to_rad"(Double) -> Double;
    MATH_EXP = "bn_rt_math_exp"(Double) -> Double;
    MATH_FABS = "bn_rt_math_fabs"(Double) -> Double;
    MATH_FLOOR = "bn_rt_math_floor"(Double) -> Double;
    MATH_FMA = "bn_rt_math_fma"(Double, Double, Double) -> Double;
    MATH_FMAX = "bn_rt_math_fmax"(Double, Double) -> Double;
    MATH_FMIN = "bn_rt_math_fmin"(Double, Double) -> Double;
    MATH_FSIGN = "bn_rt_math_fsign"(Double) -> Double;
    MATH_HYPOT = "bn_rt_math_hypot"(Double, Double) -> Double;
    MATH_GCD = "bn_rt_math_gcd"(I64, I64) -> I64;
    MATH_IABS = "bn_rt_math_iabs"(I64) -> I64;
    MATH_IMAX = "bn_rt_math_imax"(I64, I64) -> I64;
    MATH_IMIN = "bn_rt_math_imin"(I64, I64) -> I64;
    MATH_ISIGN = "bn_rt_math_isign"(I64) -> I64;
    MATH_LOG = "bn_rt_math_log"(Double) -> Double;
    MATH_LOG10 = "bn_rt_math_log10"(Double) -> Double;
    MATH_LOG2 = "bn_rt_math_log2"(Double) -> Double;
    MATH_MEAN_F64 = "bn_rt_math_mean_f64"(Ptr, I32) -> Double;
    MATH_MEAN_I32 = "bn_rt_math_mean_i32"(Ptr, I32) -> Double;
    MATH_MEDIAN_F64 = "bn_rt_math_median_f64"(Ptr, I32) -> Double;
    MATH_MEDIAN_I32 = "bn_rt_math_median_i32"(Ptr, I32) -> Double;
    MATH_MODE_F64 = "bn_rt_math_mode_f64"(Ptr, I32, Ptr) -> I32;
    MATH_MODE_I32 = "bn_rt_math_mode_i32"(Ptr, I32, Ptr) -> I32;
    MATH_POW = "bn_rt_math_pow"(Double, Double) -> Double;
    MATH_RAD_TO_DEGREE = "bn_rt_math_rad_to_degree"(Double) -> Double;
    MATH_QUARTILE1_F64 = "bn_rt_math_quartile1_f64"(Ptr, I32) -> Double;
    MATH_QUARTILE1_I32 = "bn_rt_math_quartile1_i32"(Ptr, I32) -> Double;
    MATH_QUARTILE3_F64 = "bn_rt_math_quartile3_f64"(Ptr, I32) -> Double;
    MATH_QUARTILE3_I32 = "bn_rt_math_quartile3_i32"(Ptr, I32) -> Double;
    MATH_RANGE_F64 = "bn_rt_math_range_f64"(Ptr, I32) -> Double;
    MATH_RANGE_I32 = "bn_rt_math_range_i32"(Ptr, I32) -> Double;
    MATH_ROUND = "bn_rt_math_round"(Double, Double) -> Double;
    MATH_SIN = "bn_rt_math_sin"(Double) -> Double;
    MATH_SQRT = "bn_rt_math_sqrt"(Double) -> Double;
    MATH_STDEV_F64 = "bn_rt_math_stdev_f64"(Ptr, I32) -> Double;
    MATH_STDEV_I32 = "bn_rt_math_stdev_i32"(Ptr, I32) -> Double;
    MATH_TAN = "bn_rt_math_tan"(Double) -> Double;
    MATH_TODATE = "bn_rt_math_todate"(I64, Ptr) -> I32;
    MATH_TOHOUR = "bn_rt_math_tohour"(I64) -> I32;
    MATH_TOTIME = "bn_rt_math_totime"(I64, Ptr) -> I32;
    MATH_TOTIMESTAMP = "bn_rt_math_totimestamp"(I32, I32) -> I64;
    TEMPORAL_DATE_PARSE = "bn_rt_temporal_date_parse"(Ptr, Ptr) -> I32;
    TEMPORAL_TIME_PARSE = "bn_rt_temporal_time_parse"(Ptr, Ptr) -> I32;
    TEMPORAL_TIMESTAMP_PARSE = "bn_rt_temporal_timestamp_parse"(Ptr, Ptr) -> I64;
    TEMPORAL_TIMESTAMP_FORMAT = "bn_rt_temporal_timestamp_format"(I64, Ptr) -> Ptr;
    TEMPORAL_TIMEZONE_OFFSET = "bn_rt_temporal_timezone_offset"(Ptr, I64, Ptr) -> I32;
    TEMPORAL_TIMEZONE_IS_DST = "bn_rt_temporal_timezone_is_dst"(Ptr, I64, Ptr) -> I32;
    TEMPORAL_TIMESTAMP_AT_ZONE = "bn_rt_temporal_timestamp_at_zone"(I64, Ptr, Ptr) -> Ptr;
    TEMPORAL_TIMESTAMP_FROM_LOCAL = "bn_rt_temporal_timestamp_from_local"(I32, I32, Ptr, Ptr) -> I64;
    MATH_TOWEEKDAY = "bn_rt_math_toweekday"(I64) -> I32;
    MATH_TRUNC = "bn_rt_math_trunc"(Double) -> Double;
    MATH_VAL = "bn_rt_math_val"(Ptr) -> Double;
    MATH_VARIANCE_F64 = "bn_rt_math_variance_f64"(Ptr, I32) -> Double;
    MATH_VARIANCE_I32 = "bn_rt_math_variance_i32"(Ptr, I32) -> Double;
    MATH_VMAX_F64 = "bn_rt_math_vmax_f64"(Ptr, I32, Ptr) -> Double;
    MATH_VMAX_I32 = "bn_rt_math_vmax_i32"(Ptr, I32, Ptr) -> I32;
    MATH_VMIN_F64 = "bn_rt_math_vmin_f64"(Ptr, I32, Ptr) -> Double;
    MATH_VMIN_I32 = "bn_rt_math_vmin_i32"(Ptr, I32, Ptr) -> I32;
    MATH_VECTOR_MAX_FLOAT = "bn_rt_math_vector_max_float"(Ptr, I32, I32, Ptr) -> Double;
    MATH_VECTOR_MAX_INTEGER = "bn_rt_math_vector_max_integer"(Ptr, I32, I32, Ptr) -> I64;
    MATH_VECTOR_MIN_FLOAT = "bn_rt_math_vector_min_float"(Ptr, I32, I32, Ptr) -> Double;
    MATH_VECTOR_MIN_INTEGER = "bn_rt_math_vector_min_integer"(Ptr, I32, I32, Ptr) -> I64;
    MATH_VECTOR_MODE = "bn_rt_math_vector_mode"(Ptr, I32, I32, Ptr) -> I32;
    MATH_VECTOR_STATISTIC = "bn_rt_math_vector_statistic"(Ptr, I32, I32, I32) -> Double;
    MATH_VECTOR_QUANTILE = "bn_rt_math_vector_quantile"(Ptr, I32, I32, Double) -> Double;
    MATH_VECTOR_COVARIANCE = "bn_rt_math_vector_covariance"(Ptr, I32, I32, Ptr, I32, I32, Ptr) -> Double;
    MATH_VECTOR_CORRELATION = "bn_rt_math_vector_correlation"(Ptr, I32, I32, Ptr, I32, I32, Ptr) -> Double;
    NET_ADDRESS_IS_IPV4 = "bn_rt_net_address_is_ipv4"(Ptr) -> I32;
    NET_ADDRESS_IS_IPV6 = "bn_rt_net_address_is_ipv6"(Ptr) -> I32;
    NET_ADDRESS_IS_LINK_LOCAL = "bn_rt_net_address_is_link_local"(Ptr) -> I32;
    NET_ADDRESS_IS_LOOPBACK = "bn_rt_net_address_is_loopback"(Ptr) -> I32;
    NET_ADDRESS_IS_MULTICAST = "bn_rt_net_address_is_multicast"(Ptr) -> I32;
    NET_ADDRESS_IS_PRIVATE = "bn_rt_net_address_is_private"(Ptr) -> I32;
    NET_ADDRESS_PARSE = "bn_rt_net_address_parse"(Ptr, Ptr) -> I32;
    NET_ADDRESSES_COUNT = "bn_rt_net_addresses_count"(Ptr) -> I32;
    NET_ADDRESSES_FREE = "bn_rt_net_addresses_free"(Ptr) -> Void;
    NET_ADDRESSES_GET = "bn_rt_net_addresses_get"(Ptr, I32, Ptr) -> I32;
    NET_CIDR_CONTAINS = "bn_rt_net_cidr_contains"(Ptr, I32, Ptr) -> I32;
    NET_CIDR_PARSE = "bn_rt_net_cidr_parse"(Ptr, Ptr, Ptr) -> I32;
    NET_HANDLE_CLOSE = "bn_rt_net_handle_close"(I64) -> I32;
    NET_NEIGHBOR = "bn_rt_net_neighbor"(Ptr, Ptr) -> I32;
    NET_PING = "bn_rt_net_ping"(Ptr, I32, Ptr, Ptr) -> I32;
    NET_RESOLVE = "bn_rt_net_resolve"(Ptr, I32, Ptr) -> I32;
    NET_REVERSE = "bn_rt_net_reverse"(Ptr, I32, Ptr) -> I32;
    NET_TCP_ACCEPT = "bn_rt_net_tcp_accept"(I64, I32, Ptr) -> I32;
    NET_TCP_CONNECT = "bn_rt_net_tcp_connect"(Ptr, I32, I32, Ptr) -> I32;
    NET_TCP_LISTEN_WITH_BACKLOG = "bn_rt_net_tcp_listen_with_backlog"(Ptr, I32, I32, Ptr) -> I32;
    NET_TCP_LISTENER_LOCAL_ENDPOINT = "bn_rt_net_tcp_listener_local_endpoint"(I64, Ptr, Ptr) -> I32;
    NET_TCP_READ = "bn_rt_net_tcp_read"(I64, Ptr, I32, Ptr) -> I32;
    NET_TCP_SET_TIMEOUTS = "bn_rt_net_tcp_set_timeouts"(I64, I32, I32) -> I32;
    NET_TCP_SHUTDOWN_READ = "bn_rt_net_tcp_shutdown_read"(I64) -> I32;
    NET_TCP_SHUTDOWN_WRITE = "bn_rt_net_tcp_shutdown_write"(I64) -> I32;
    NET_TCP_STREAM_LOCAL_ENDPOINT = "bn_rt_net_tcp_stream_local_endpoint"(I64, Ptr, Ptr) -> I32;
    NET_TCP_STREAM_REMOTE_ENDPOINT = "bn_rt_net_tcp_stream_remote_endpoint"(I64, Ptr, Ptr) -> I32;
    NET_TCP_WRITE = "bn_rt_net_tcp_write"(I64, Ptr, I32, Ptr) -> I32;
    NET_UDP_BIND = "bn_rt_net_udp_bind"(Ptr, I32, Ptr) -> I32;
    NET_UDP_LOCAL_ENDPOINT = "bn_rt_net_udp_local_endpoint"(I64, Ptr, Ptr) -> I32;
    NET_UDP_PACKET_COPY_TO = "bn_rt_net_udp_packet_copy_to"(I64, Ptr, I32, Ptr) -> I32;
    NET_UDP_PACKET_SIZE = "bn_rt_net_udp_packet_size"(I64) -> I32;
    NET_UDP_PACKET_SOURCE = "bn_rt_net_udp_packet_source"(I64, Ptr, Ptr) -> I32;
    NET_UDP_PACKET_TRUNCATED = "bn_rt_net_udp_packet_truncated"(I64) -> I32;
    NET_UDP_RECEIVE_HANDLE = "bn_rt_net_udp_receive_handle"(I64, I32, I32, Ptr) -> I32;
    NET_UDP_SEND_TO = "bn_rt_net_udp_send_to"(I64, Ptr, I32, Ptr, I32, Ptr) -> I32;
    POLICY_CHECK = "bn_rt_policy_check"(I32) -> Void;
    POLICY_FILESYSTEM_ROOT = "bn_rt_policy_filesystem_root"(I32, Ptr) -> I32;
    POLICY_FILESYSTEM_SANDBOXED = "bn_rt_policy_filesystem_sandboxed"() -> I32;
    POLICY_INIT = "bn_rt_policy_init"(I32, I64) -> I32;
    PRINT_DATE = "bn_rt_print_date"(I32) -> Void;
    PRINT_FLOAT = "bn_rt_print_float"(Double) -> Void;
    PRINT_FLOAT32 = "bn_rt_print_float32"(Double) -> Void;
    PRINT_STR = "bn_rt_print_str"(Ptr, I64) -> Void;
    PRINT_TIME = "bn_rt_print_time"(I32) -> Void;
    PUTCHAR = "putchar"(I32) -> I32;
    RANDOM_NEXT = "bn_rt_random_next"() -> Double;
    RANDOM_SEED = "bn_rt_random_seed"(I64) -> I32;
    SETMODE = "_setmode"(I32, I32) -> I32;
    SQLITE_BEGIN = "bn_rt_sqlite_begin"(I64) -> I32;
    SQLITE_CHANGES = "bn_rt_sqlite_changes"(I64) -> I64;
    SQLITE_CLOSE = "bn_rt_sqlite_close"(I64) -> I32;
    SQLITE_COMMIT = "bn_rt_sqlite_commit"(I64) -> I32;
    SQLITE_EXEC = "bn_rt_sqlite_exec"(I64, Ptr) -> I32;
    SQLITE_LAST_INSERT_ROWID = "bn_rt_sqlite_last_insert_rowid"(I64) -> I64;
    SQLITE_OPEN = "bn_rt_sqlite_open"(Ptr, Ptr) -> I32;
    SQLITE_OPEN_EXISTING = "bn_rt_sqlite_open_existing"(Ptr, Ptr) -> I32;
    SQLITE_OPEN_READ_ONLY = "bn_rt_sqlite_open_read_only"(Ptr, Ptr) -> I32;
    SQLITE_QUERY = "bn_rt_sqlite_query"(I64, Ptr, Ptr) -> I32;
    SQLITE_RETAIN = "bn_rt_sqlite_retain"(I64) -> I32;
    SQLITE_ROLLBACK = "bn_rt_sqlite_rollback"(I64) -> I32;
    STACK_CHECK = "bn_rt_stack_check"(Ptr) -> I32;
    STR_ASC = "bn_rt_str_asc"(Ptr) -> I64;
    STR_CHAR_UTF8 = "bn_rt_str_char_utf8"(I64) -> I64;
    STR_CONCAT = "bn_rt_str_concat"(Ptr, I64, Ptr, I64) -> Ptr;
    STR_CMP = "bn_rt_str_cmp"(Ptr, Ptr) -> I32;
    STR_EQ = "bn_rt_str_eq"(Ptr, Ptr) -> I32;
    STR_FROM_CHAR = "bn_rt_str_from_char"(I64) -> Ptr;
    STR_FROM_PACKED = "bn_rt_str_from_packed"(I64) -> Ptr;
    STR_INDEX_UTF8 = "bn_rt_str_index_utf8"(Ptr, I32, Ptr) -> I64;
    STR_CSTR_COPY = "bn_rt_cstr_copy"(Ptr) -> Ptr;
    STR_INPUT = "bn_rt_input_line"(Ptr) -> Ptr;
    STR_LEN = "bn_rt_str_len"(Ptr) -> I32;
    STR_RELEASE = "bn_rt_str_release"(Ptr) -> I32;
    STR_RETAIN = "bn_rt_str_retain"(Ptr) -> I32;
    STR_TO_LOWER = "bn_rt_str_to_lower"(Ptr) -> Ptr;
    STR_TO_UPPER = "bn_rt_str_to_upper"(Ptr) -> Ptr;
    TEXT_FLOAT = "bn_rt_text_float"(Double) -> Ptr;
    TEXT_FLOAT32 = "bn_rt_text_float32"(Double) -> Ptr;
    TEXT_INT = "bn_rt_text_int"(I64) -> Ptr;
    TEXT_UINT = "bn_rt_text_uint"(I64) -> Ptr;
    TRAP_REPORT = "bn_rt_trap_report"(Ptr, I64, I64, I64, I64) -> Void;
    TRAP_REPORT_FAILURE = "bn_rt_trap_report_failure"(Ptr) -> Void;
    UNLOCK_FILE = "_unlock_file"(Ptr) -> Void;
    VECTOR_NEST = "bn_rt_vector_nest"(Ptr, Ptr, Ptr, Ptr, I32, I64) -> Void;
    FFI_NEW_RECORD = "bn_rt_ffi_new_record"(I32) -> I64;
    FFI_RELEASE = "bn_rt_ffi_release"(I64) -> Void;
    FFI_SET_INT8 = "bn_rt_ffi_set_int8"(I64, I8) -> Void;
    FFI_SET_UINT8 = "bn_rt_ffi_set_uint8"(I64, I8) -> Void;
    FFI_SET_INT16 = "bn_rt_ffi_set_int16"(I64, I16) -> Void;
    FFI_SET_UINT16 = "bn_rt_ffi_set_uint16"(I64, I16) -> Void;
    FFI_SET_INT32 = "bn_rt_ffi_set_int32"(I64, I32) -> Void;
    FFI_SET_UINT32 = "bn_rt_ffi_set_uint32"(I64, I32) -> Void;
    FFI_SET_INT64 = "bn_rt_ffi_set_int64"(I64, I64) -> Void;
    FFI_SET_UINT64 = "bn_rt_ffi_set_uint64"(I64, I64) -> Void;
    FFI_SET_FLOAT = "bn_rt_ffi_set_float"(I64, Float) -> Void;
    FFI_SET_DOUBLE = "bn_rt_ffi_set_double"(I64, Double) -> Void;
    FFI_SET_BOOL = "bn_rt_ffi_set_bool"(I64, I32) -> Void;
    FFI_SET_STRING = "bn_rt_ffi_set_string"(I64, Ptr) -> Void;
    FFI_SET_ARRAY = "bn_rt_ffi_set_array"(I64, Ptr, I64, I64) -> Void;
    FFI_GET_INT8 = "bn_rt_ffi_get_int8"(I64) -> I8;
    FFI_GET_UINT8 = "bn_rt_ffi_get_uint8"(I64) -> I8;
    FFI_GET_INT16 = "bn_rt_ffi_get_int16"(I64) -> I16;
    FFI_GET_UINT16 = "bn_rt_ffi_get_uint16"(I64) -> I16;
    FFI_GET_INT32 = "bn_rt_ffi_get_int32"(I64) -> I32;
    FFI_GET_UINT32 = "bn_rt_ffi_get_uint32"(I64) -> I32;
    FFI_GET_INT64 = "bn_rt_ffi_get_int64"(I64) -> I64;
    FFI_GET_UINT64 = "bn_rt_ffi_get_uint64"(I64) -> I64;
    FFI_GET_FLOAT = "bn_rt_ffi_get_float"(I64) -> Float;
    FFI_GET_DOUBLE = "bn_rt_ffi_get_double"(I64) -> Double;
    FFI_GET_BOOL = "bn_rt_ffi_get_bool"(I64) -> I32;
    FFI_ARGS_NEW = "bn_rt_ffi_args_new"() -> I64;
    FFI_ARGS_ADD = "bn_rt_ffi_args_add"(I64, I64) -> Void;
    FFI_ARGS_COUNT = "bn_rt_ffi_args_count"(I64) -> I32;
    FFI_CALL = "bn_rt_ffi_call"(Ptr, Ptr, I64, I64) -> I32;
    FFI_HANDLE_RESULT_VALUE = "bn_rt_ffi_handle_result_value"(I64) -> Ptr;
    FFI_HANDLE_READ_STRING = "bn_rt_ffi_handle_read_string"(I64, Ptr) -> I32;
    FFI_ARRAY_READ_FROM = "bn_rt_ffi_array_read_from"(I64, I64) -> I32;
    FFI_ARRAY_WRITE_TO = "bn_rt_ffi_array_write_to"(I64, I64) -> I32;
}

/// `printf`, the one variadic function the backend calls.
pub(crate) const PRINTF: VariadicFn<1> = VariadicFn {
    name: "printf",
    ret: I32,
    params: [Ptr],
};

declarations! {
    BN_RT_DECLARATIONS =
    POLICY_INIT POLICY_CHECK POLICY_FILESYSTEM_SANDBOXED POLICY_FILESYSTEM_ROOT ENV_GET ENV_HAS
    EXEC_RUN EXEC_RESULT_RETURN_CODE EXEC_RESULT_STDOUT EXEC_RESULT_STDERR EXEC_RESULT_CLOSE
    DATAFRAME_CREATE DATAFRAME_ROW_COUNT DATAFRAME_COLUMN_COUNT DATAFRAME_ADD_INTEGER_START
    DATAFRAME_SET_INTEGER_CELL DATAFRAME_ADD_FLOAT DATAFRAME_ADD_BOOLEAN DATAFRAME_ADD_STRING
    DATAFRAME_GET_INTEGER DATAFRAME_GET_FLOAT DATAFRAME_GET_BOOLEAN DATAFRAME_GET_STRING
    DATAFRAME_SET_LABEL DATAFRAME_REDUCE DATAFRAME_ZSCORE DATAFRAME_COPY_INTEGER
    DATAFRAME_COPY_FLOAT DATAFRAME_SELECT DATAFRAME_SLICE DATAFRAME_TRANSPOSE
    DATAFRAME_APPEND_ROWS DATAFRAME_APPEND_COLUMNS DATAFRAME_JOIN DATAFRAME_CONVERT_INTEGER
    DATAFRAME_CONVERT_FLOAT DATAFRAME_COUNT_NA DATAFRAME_DROP_NA DATAFRAME_FILL_NA
    DATAFRAME_COLUMN_NAME_OWNED DATAFRAME_CLOSE DATAFRAME_RETAIN FILE_OPEN FILE_NEW
    FILE_CLOSE FILE_RELEASE FILE_RETAIN FILE_READ_ALL FILE_READ_LINE FILE_WRITE FILE_WRITE_LINE
    FILE_READ_BYTES FILE_WRITE_BYTES FS_EXISTS FS_DELETE_FILE DATAFRAME_READ_CSV
    DATAFRAME_WRITE_CSV LOG_CLIENT_ACCESS LOG_LOGGER_NEW LOG_LOGGER_WRITE LOG_LOGGER_ACCESS LOG_LOGGER_SET_LEVEL
    LOG_LOGGER_SET_DEFAULTS LOG_LOGGER_SET_ACCESS_DEFAULTS LOG_LOGGER_FLUSH LOG_LOGGER_CLOSE
    LOG_LOGGER_DELETE LOG_LOGGER_RETAIN STACK_CHECK VECTOR_NEST CLOCK_NOW CLOCK_TIMER
    RANDOM_SEED RANDOM_NEXT CONSOLE_CLS CONSOLE_BEEP CONSOLE_PRINT_AT CONSOLE_NUM_COLS
    CONSOLE_NUM_ROWS STR_ASC STR_CONCAT STR_TO_LOWER STR_TO_UPPER PRINT_STR TEXT_INT TEXT_UINT TEXT_FLOAT
    TEXT_FLOAT32 CRYPTO_SHA256 CRYPTO_SHA512 CRYPTO_BYTES_FROM_TEXT CRYPTO_BYTES_FROM_HEX
    CRYPTO_BYTES_LENGTH CRYPTO_BYTES_TO_HEX CRYPTO_BYTES_RELEASE CRYPTO_BYTES_RETAIN CRYPTO_SEAL CRYPTO_OPEN
    CRYPTO_HMAC CRYPTO_HMAC_VERIFY CRYPTO_ARGON2ID CRYPTO_PUBLIC_KEY CRYPTO_SIGN CRYPTO_VERIFY
    CRYPTO_SLICE CRYPTO_KEM_KEYPAIR CRYPTO_KEM_ENCAPSULATE CRYPTO_KEM_DECAPSULATE
    CRYPTO_DSA_KEYPAIR CRYPTO_DSA_SIGN CRYPTO_DSA_VERIFY
    CRYPTO_GENERATE_KEY CRYPTO_GENERATE_NONCE CRYPTO_IMPORT_KEY CRYPTO_KEY_ALGORITHM
    CRYPTO_KEY_LENGTH CRYPTO_KEY_RELEASE CRYPTO_KEY_RETAIN CRYPTO_KEY_ZEROIZE CRYPTO_RANDOM_BYTES
    COMPRESS_DEFLATE COMPRESS_INFLATE COMPRESS_GZIP COMPRESS_GUNZIP COMPRESS_DEFLATE_TEXT
    COMPRESS_INFLATE_TEXT COMPRESS_GZIP_TEXT COMPRESS_GUNZIP_TEXT COMPRESS_BYTES_FROM_HEX
    COMPRESS_BYTES_FROM_TEXT COMPRESS_BYTES_LENGTH COMPRESS_BYTES_RELEASE COMPRESS_BYTES_RETAIN
    COMPRESS_BYTES_TO_HEX COMPRESS_BYTES_TO_TEXT ZIP_CREATE ZIP_OPEN ZIP_ADD_FILE ZIP_ADD_TEXT
    ZIP_ADD_STORED_TEXT ZIP_ADD_BYTES ZIP_ADD_STORED_BYTES ZIP_EXTRACT_ALL ZIP_EXTRACT_FILE
    ZIP_EXTRACT_TEXT ZIP_EXTRACT_BYTES ZIP_ENTRY_COUNT ZIP_ENTRY_NAME ZIP_CLOSE ZIP_RETAIN
    ZIP_RELEASE
    SQLITE_OPEN SQLITE_OPEN_READ_ONLY
    SQLITE_OPEN_EXISTING SQLITE_CLOSE SQLITE_RETAIN SQLITE_EXEC SQLITE_QUERY SQLITE_BEGIN SQLITE_COMMIT
    SQLITE_ROLLBACK SQLITE_CHANGES SQLITE_LAST_INSERT_ROWID JSON_OBJECT JSON_PARSE
    JSON_STRINGIFY JSON_SET_STRING JSON_GET_STRING JSON_RELEASE JSON_RETAIN JSON_ARRAY JSON_KIND JSON_HAS
    JSON_LENGTH JSON_SET_INTEGER JSON_SET_BOOLEAN JSON_SET_NULL JSON_GET_INTEGER
    JSON_GET_BOOLEAN JSON_SET_FLOAT JSON_GET_FLOAT JSON_SET_JSON JSON_GET_JSON JSON_CLONE
    JSON_APPEND_STRING JSON_APPEND_INTEGER JSON_APPEND_FLOAT JSON_APPEND_BOOLEAN
    JSON_APPEND_NULL JSON_APPEND_JSON JSON_GET_STRING_AT JSON_GET_INTEGER_AT JSON_GET_FLOAT_AT
    JSON_GET_BOOLEAN_AT JSON_GET_JSON_AT JSON_SET_STRING_AT JSON_SET_INTEGER_AT
    JSON_SET_FLOAT_AT JSON_SET_BOOLEAN_AT JSON_SET_NULL_AT JSON_SET_JSON_AT STR_CHAR_UTF8
    STR_RELEASE STR_RETAIN
    NET_ADDRESS_PARSE NET_ADDRESS_IS_IPV4 NET_ADDRESS_IS_IPV6 NET_ADDRESS_IS_LOOPBACK
    NET_ADDRESS_IS_PRIVATE NET_ADDRESS_IS_LINK_LOCAL NET_ADDRESS_IS_MULTICAST NET_CIDR_PARSE
    NET_CIDR_CONTAINS NET_PING NET_REVERSE NET_NEIGHBOR NET_RESOLVE NET_ADDRESSES_COUNT
    NET_ADDRESSES_GET NET_ADDRESSES_FREE NET_UDP_BIND NET_TCP_CONNECT
    NET_TCP_LISTEN_WITH_BACKLOG NET_TCP_ACCEPT NET_TCP_LISTENER_LOCAL_ENDPOINT
    NET_UDP_LOCAL_ENDPOINT NET_TCP_STREAM_LOCAL_ENDPOINT NET_TCP_STREAM_REMOTE_ENDPOINT
    NET_TCP_WRITE NET_TCP_READ NET_TCP_SET_TIMEOUTS NET_TCP_SHUTDOWN_READ NET_TCP_SHUTDOWN_WRITE
    NET_HANDLE_CLOSE NET_UDP_SEND_TO NET_UDP_RECEIVE_HANDLE NET_UDP_PACKET_SIZE
    NET_UDP_PACKET_TRUNCATED NET_UDP_PACKET_COPY_TO NET_UDP_PACKET_SOURCE DISPATCH_QUEUE_CREATE
    DISPATCH_QUEUE_CREATE_AUTO DISPATCH_SUBMIT DISPATCH_AWAIT DISPATCH_CANCEL
    DISPATCH_TICKET_CLOSE DISPATCH_TICKET_CANCEL DISPATCH_TICKET_ID DISPATCH_TICKET_STATUS
    DISPATCH_TICKET_IS_DONE DISPATCH_TICKET_ERROR DISPATCH_QUEUE_JOIN DISPATCH_QUEUE_CLOSE
    DISPATCH_GROUP_CREATE DISPATCH_GROUP_ENTER DISPATCH_GROUP_LEAVE DISPATCH_GROUP_WAIT
    DISPATCH_GROUP_CLOSE DISPATCH_BARRIER_CREATE DISPATCH_BARRIER_WAIT DISPATCH_BARRIER_CLOSE
    DISPATCH_SEMAPHORE_CREATE DISPATCH_SEMAPHORE_ACQUIRE DISPATCH_SEMAPHORE_RELEASE
    DISPATCH_SEMAPHORE_CLOSE DISPATCH_MUTEX_CREATE DISPATCH_MUTEX_LOCK DISPATCH_MUTEX_UNLOCK
    DISPATCH_MUTEX_CLOSE HOST_NUM_PROCS
    FFI_NEW_RECORD FFI_RELEASE FFI_SET_INT8 FFI_SET_UINT8 FFI_SET_INT16 FFI_SET_UINT16
    FFI_SET_INT32 FFI_SET_UINT32 FFI_SET_INT64 FFI_SET_UINT64 FFI_SET_FLOAT FFI_SET_DOUBLE
    FFI_SET_BOOL FFI_SET_STRING FFI_SET_ARRAY FFI_GET_INT8 FFI_GET_UINT8 FFI_GET_INT16
    FFI_GET_UINT16 FFI_GET_INT32 FFI_GET_UINT32 FFI_GET_INT64 FFI_GET_UINT64 FFI_GET_FLOAT
    FFI_GET_DOUBLE FFI_GET_BOOL FFI_ARGS_NEW FFI_ARGS_ADD FFI_ARGS_COUNT FFI_CALL
    FFI_HANDLE_RESULT_VALUE FFI_HANDLE_READ_STRING FFI_ARRAY_READ_FROM FFI_ARRAY_WRITE_TO
    ;
    BN_RT_MATH_DECLARATIONS =
    MATH_IABS MATH_ISIGN MATH_IMIN MATH_IMAX MATH_TOHOUR MATH_TOWEEKDAY MATH_VAL MATH_FABS
    MATH_FSIGN MATH_FLOOR MATH_CEIL MATH_TRUNC MATH_EXP MATH_LOG MATH_LOG10 MATH_LOG2 MATH_SIN
    MATH_COS MATH_TAN MATH_ASIN MATH_ACOS MATH_ATAN MATH_SQRT MATH_DEGREE_TO_RAD MATH_RAD_TO_DEGREE MATH_POW
    MATH_ATAN2 MATH_HYPOT MATH_GCD
    MATH_FMIN MATH_FMAX MATH_ROUND MATH_FMA MATH_VMIN_I32 MATH_VMAX_I32 MATH_VMIN_F64
    MATH_VMAX_F64 MATH_MEAN_I32 MATH_MEDIAN_I32 MATH_QUARTILE1_I32 MATH_QUARTILE3_I32
    MATH_RANGE_I32 MATH_STDEV_I32 MATH_VARIANCE_I32 MATH_MODE_I32 MATH_MEAN_F64 MATH_MEDIAN_F64
    MATH_QUARTILE1_F64 MATH_QUARTILE3_F64 MATH_RANGE_F64 MATH_STDEV_F64 MATH_VARIANCE_F64
    MATH_MODE_F64 MATH_TODATE MATH_TOTIME MATH_TOTIMESTAMP MATH_VECTOR_STATISTIC MATH_VECTOR_QUANTILE MATH_VECTOR_COVARIANCE
    MATH_VECTOR_CORRELATION MATH_VECTOR_MODE
    MATH_VECTOR_MIN_INTEGER MATH_VECTOR_MAX_INTEGER MATH_VECTOR_MIN_FLOAT MATH_VECTOR_MAX_FLOAT
    TEMPORAL_DATE_PARSE TEMPORAL_TIME_PARSE TEMPORAL_TIMESTAMP_PARSE TEMPORAL_TIMESTAMP_FORMAT
    TEMPORAL_TIMEZONE_OFFSET TEMPORAL_TIMEZONE_IS_DST TEMPORAL_TIMESTAMP_AT_ZONE TEMPORAL_TIMESTAMP_FROM_LOCAL
    ;
    STRING_CONCAT_DECLARATIONS =
    STR_CONCAT MALLOC LLVM_MEMCPY_P0_P0_I64
    ;
    ARC_DECLARATIONS = ARC_REGISTER ARC_RETAIN ARC_RELEASE ARC_FINISH_DESTROY ARC_ADDRESS;
    PRINT_DECLARATIONS = PRINTF PUTCHAR PRINT_STR;
    PRINT_FLOAT_DECLARATIONS = PRINT_FLOAT PRINT_FLOAT32;
    PRINT_TEMPORAL_DECLARATIONS = PRINT_DATE PRINT_TIME;
    STRING_OPS_DECLARATIONS = STR_LEN STR_INDEX_UTF8 STR_CMP STR_EQ STR_FROM_CHAR STR_FROM_PACKED STR_CONCAT STR_CSTR_COPY STR_RELEASE STR_RETAIN;
    HEAP_COPY_DECLARATIONS = MALLOC LLVM_MEMCPY_P0_P0_I64;
    INPUT_DECLARATIONS = STR_INPUT;
    WINDOWS_STDIO_LOCK_DECLARATIONS = ACRT_IOB_FUNC LOCK_FILE UNLOCK_FILE;
    POSIX_STDIO_LOCK_DECLARATIONS = FLOCKFILE FUNLOCKFILE;
}

/// `memcmp`: byte equality of a long integer vector (`composite_compare`).
pub(crate) fn memcmp() -> Intrinsic {
    let params = vec![LlvmType::Ptr, LlvmType::Ptr, LlvmType::I64];
    Intrinsic {
        name: "memcmp",
        ret: LlvmType::I32,
        params,
    }
}

/// `llvm.memmove.p0.p0.i64`: the copy of a fixed vector's elements, a move
/// because a vector may be stored into the slot it came from.
pub(crate) fn memmove() -> Intrinsic {
    let params = vec![LlvmType::Ptr, LlvmType::Ptr, LlvmType::I64, LlvmType::I1];
    Intrinsic {
        name: "llvm.memmove.p0.p0.i64",
        ret: LlvmType::Void,
        params,
    }
}

/// `llvm.memset.p0.i64`: zero-initialization of dynamic vector buffers.
pub(crate) fn memset() -> Intrinsic {
    let params = vec![LlvmType::Ptr, LlvmType::I8, LlvmType::I64, LlvmType::I1];
    Intrinsic {
        name: "llvm.memset.p0.i64",
        ret: LlvmType::Void,
        params,
    }
}

/// `llvm.frameaddress.p0`: the current frame, for the stack check.
pub(crate) fn frame_address() -> Intrinsic {
    let params = vec![LlvmType::I32];
    Intrinsic {
        name: "llvm.frameaddress.p0",
        ret: LlvmType::Ptr,
        params,
    }
}

/// `{ T, i1 } @name(T, T)`: an integer operation that reports overflow.
pub(crate) fn overflow(name: &'static str, ty: LlvmType) -> Intrinsic {
    let ret = LlvmType::Struct(vec![ty.clone(), LlvmType::I1]);
    Intrinsic {
        name,
        ret,
        params: vec![ty.clone(), ty],
    }
}

/// `T @name(T, T)`: a floating-point power.
pub(crate) fn float_power(name: &'static str, ty: LlvmType) -> Intrinsic {
    Intrinsic {
        name,
        ret: ty.clone(),
        params: vec![ty.clone(), ty],
    }
}

#[cfg(test)]
mod tests {
    use super::{BN_RT_DECLARATIONS, PRINTF, memmove, overflow};
    use crate::ir::{Declaration, LlvmType};

    /// Bucket 0.6.5 T.5.1 and T.5.5: a `declare` comes from this table only, and
    /// no backend source writes LLVM text (a definition, a global, a debug
    /// attachment); `LlvmModule` takes typed pieces only.
    #[test]
    fn no_backend_source_writes_llvm_text() {
        let text_forms = [
            "declare ",
            "define ",
            "!dbg",
            "unnamed_addr",
            "thread_local",
            "external global",
            "private ",
            "internal ",
            "= global",
            "constant [",
        ];
        // The emitter is `lib.rs`, any other file at the top of `src/` and
        // `src/llvm/`; only `src/ir/` renders text.
        let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut sources = Vec::new();
        for directory in [source_root.clone(), source_root.join("llvm")] {
            for entry in std::fs::read_dir(directory).expect("read the backend sources") {
                let path = entry.expect("source entry").path();
                if path.extension().is_some_and(|extension| extension == "rs") {
                    sources.push(path);
                }
            }
        }
        assert!(
            sources.iter().any(|path| path.ends_with("lib.rs")),
            "the gate reads the backend sources"
        );
        for path in sources {
            let source = std::fs::read_to_string(&path).expect("read a backend source");
            let code = source.split("#[cfg(test)]").next().unwrap_or_default();
            // Only string literals can carry LLVM text; the odd pieces of a line
            // split at its quotes are the literal contents.
            let written = code
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .find(|line| {
                    let literals = line.split('"').skip(1).step_by(2);
                    literals
                        .into_iter()
                        .any(|literal| text_forms.iter().any(|form| literal.contains(form)))
                });
            assert!(
                written.is_none(),
                "{} writes LLVM text: {written:?}",
                path.display()
            );
        }
    }

    #[test]
    fn signatures_render_their_declarations() {
        assert_eq!(
            BN_RT_DECLARATIONS[0].declaration(),
            "declare i32 @bn_rt_policy_init(i32, i64)"
        );
        assert_eq!(PRINTF.declaration(), "declare i32 @printf(ptr, ...)");
        let checked = overflow("llvm.sadd.with.overflow.i8", LlvmType::I8);
        assert_eq!(
            checked.declaration(),
            "declare { i8, i1 } @llvm.sadd.with.overflow.i8(i8, i8)"
        );
        assert!(memmove() < checked, "intrinsics order by their declaration");
    }
}
