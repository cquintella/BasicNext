; Basic Next 0.2
@.bn_fmt_int = private unnamed_addr constant [5 x i8] c"%lld\00"
@.bn_fmt_uint = private unnamed_addr constant [5 x i8] c"%llu\00"
@.bn_fmt_float = private unnamed_addr constant [6 x i8] c"%.17g\00"
@.bn_fmt_str = private unnamed_addr constant [3 x i8] c"%s\00"
@.bn_true = private unnamed_addr constant [5 x i8] c"TRUE\00"
@.bn_false = private unnamed_addr constant [6 x i8] c"FALSE\00"
@.bn_empty = private unnamed_addr constant [1 x i8] c"\00"
@.bn_eof = private unnamed_addr constant [4 x i8] c"EOF\00"

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
  br i1 %ovf2, label %trap_numeric_overflow, label %b0.cont0
b0.cont0:
  ret i32 %v2
trap_numeric_overflow:
  ret i32 1
}
