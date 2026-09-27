#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_bndata_convert(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let receiver = arguments[0];
    if llvm_type(
        analysis
            .values
            .get(&receiver)
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfconvhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            receiver.0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfconvhandle{dest} = ptrtoint ptr %v{} to i64",
            receiver.0
        );
    }
    let _ = writeln!(
        text,
        "  %dfconvrc{dest} = call i32 @{symbol}(i64 %dfconvhandle{dest}, ptr %v{})",
        arguments[1].0
    );
    let _ = writeln!(text, "  %dfconverr{dest} = icmp ne i32 %dfconvrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %dfconvmsg{dest} = select i1 %dfconverr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfconvagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfconverr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfconvagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfconvagg0{dest}, ptr %dfconvmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfconvagg1{dest}, i64 0, 2"
    );
}

pub(crate) fn lower_bndata_slice(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let handle = arguments[0];
    let _ = writeln!(
        text,
        "  %dfslicehandle{dest} = ptrtoint ptr %v{} to i64",
        handle.0
    );
    let mut operands = Vec::new();
    for (index, argument) in arguments[1..].iter().enumerate() {
        operands.push(coerce_to_type(
            text,
            *argument,
            analysis
                .values
                .get(argument)
                .expect("validated slice bound"),
            &Type::Integer(IntegerType::Int32),
        ));
        let _ = index;
    }
    let _ = writeln!(text, "  %dfsliceout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dfsrc{dest} = call i32 @bn_rt_dataframe_slice(i64 %dfslicehandle{dest}, i32 {}, i32 {}, i32 {}, i32 {}, ptr %dfsliceout{dest})",
        operands[0], operands[1], operands[2], operands[3]
    );
    let _ = writeln!(text, "  %dfslicerr{dest} = icmp ne i32 %dfsrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %dfslicevalue{dest} = load i64, ptr %dfsliceout{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfsliceagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfslicerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfsliceagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfsliceagg0{dest}, ptr @.bn_dataframe_error, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfsliceagg1{dest}, i64 %dfslicevalue{dest}, 2"
    );
}
