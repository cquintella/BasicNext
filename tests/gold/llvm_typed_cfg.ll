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

define i32 @main(i32 %argc, ptr %argv) {
b0:
  %v0 = add i32 0, 1
  %v1 = add i8 0, 2
  %v2 = add i64 0, 3
  %v3 = fadd float 0.0, 1.5
  %v4 = fadd double 0.0, 2.5
  %v5 = or i1 0, 1
  br i1 %v5, label %b1, label %b2
b1:
  br label %b3
b2:
  br label %b3
b3:
  ret i32 0
}
