; Basic Next 0.2
@.bn_fmt_int = private unnamed_addr constant [5 x i8] c"%lld\00"
@.bn_fmt_uint = private unnamed_addr constant [5 x i8] c"%llu\00"
@.bn_fmt_float = private unnamed_addr constant [6 x i8] c"%.17g\00"
@.bn_fmt_str = private unnamed_addr constant [3 x i8] c"%s\00"
@.bn_fmt_error = private unnamed_addr constant [16 x i8] c"Error(%lld, %s)\00"
@.bn_asc_error = private unnamed_addr constant [32 x i8] c"ASC requires a non-empty STRING\00"
@.bn_char_error = private unnamed_addr constant [34 x i8] c"CHAR code is not a Unicode scalar\00"
@.bn_dataframe_error = private unnamed_addr constant [25 x i8] c"DataFrame column failure\00"
@.bn_dataframe_duplicate = private unnamed_addr constant [22 x i8] c"duplicate column name\00"
@.bn_dataframe_length = private unnamed_addr constant [23 x i8] c"column length mismatch\00"
@.bn_dataframe_index = private unnamed_addr constant [27 x i8] c"column index out of bounds\00"
@.bn_true = private unnamed_addr constant [5 x i8] c"TRUE\00"
@.bn_false = private unnamed_addr constant [6 x i8] c"FALSE\00"
@.bn_empty = private unnamed_addr constant [1 x i8] c"\00"
@.bn_eof = private constant [4 x i8] c"EOF\00"

declare i32 @printf(ptr, ...)
declare i32 @putchar(i32)
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

define i32 @main(i32 %argc, ptr %argv) {
b0:
  %v0 = add i32 0, 2147483647
  %v1 = add i32 0, 1
  %ov2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v0, i32 %v1)
  %v2 = extractvalue { i32, i1 } %ov2, 0
  %ovf2 = extractvalue { i32, i1 } %ov2, 1
  %ovl2 = sext i32 %v0 to i128
  %ovr2 = sext i32 %v1 to i128
  %ovexact2 = add i128 %ovl2, %ovr2
  br i1 %ovf2, label %b0.cont1, label %b0.cont0
b0.cont1:
  %b0.cont1.fact0.lo = trunc i128 %ovexact2 to i64
  %b0.cont1.fact0.shr = lshr i128 %ovexact2, 64
  %b0.cont1.fact0.hi = trunc i128 %b0.cont1.fact0.shr to i64
  %b0.cont1.fact1.lo = trunc i128 0 to i64
  %b0.cont1.fact1.shr = lshr i128 0, 64
  %b0.cont1.fact1.hi = trunc i128 %b0.cont1.fact1.shr to i64
  call void @bn_rt_trap_report(ptr @.bn_trap_4e554d455249435f4f564552464c4f571f301f301f301f311f311f301f301f301f311f311f6f7065726174696f6e1e636f6e76657274696e6720013020746f20494e543332, i64 %b0.cont1.fact0.lo, i64 %b0.cont1.fact0.hi, i64 %b0.cont1.fact1.lo, i64 %b0.cont1.fact1.hi)
  br label %trap_numeric_overflow
b0.cont0:
  ret i32 %v2
trap_numeric_overflow:
  ret i32 1
}
@.bn_trap_4e554d455249435f4f564552464c4f571f301f301f301f311f311f301f301f301f311f311f6f7065726174696f6e1e636f6e76657274696e6720013020746f20494e543332 = private unnamed_addr constant [48 x i8] c"error[NUMERIC_OVERFLOW]: converting \010 to INT32\00"
define void @bn_rt_trap_report(ptr %text, i64 %a, i64 %b, i64 %c, i64 %d) {
  ret void
}
