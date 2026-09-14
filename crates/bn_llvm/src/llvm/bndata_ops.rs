#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_bndata_set_label(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %dflabelhandle{dest} = ptrtoint ptr %v{} to i64",
        arguments[0].0
    );
    let _ = writeln!(
        text,
        "  %dflabelrc{dest} = call i32 @bn_rt_dataframe_set_label(i64 %dflabelhandle{dest}, ptr %v{}, ptr %v{})",
        arguments[1].0, arguments[2].0
    );
    let _ = writeln!(
        text,
        "  %dflabelerr{dest} = icmp ne i32 %dflabelrc{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dflabelmsg{dest} = select i1 %dflabelerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dflabelagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dflabelerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dflabelagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dflabelagg0{dest}, ptr %dflabelmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dflabelagg1{dest}, i64 0, 2"
    );
}

pub(crate) fn lower_bndata_reduce(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    operation: u32,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let name = arguments[1];
    let name_operand = format!("%v{}", name.0);
    if llvm_type(
        analysis
            .values
            .get(&arguments[0])
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfredhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            arguments[0].0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfredhandle{dest} = ptrtoint ptr %v{} to i64",
            arguments[0].0
        );
    }
    let _ = writeln!(text, "  %dfredout{dest} = alloca double");
    let _ = writeln!(text, "  %dfredna{dest} = alloca i8");
    let _ = writeln!(
        text,
        "  %dfredrc{dest} = call i32 @bn_rt_dataframe_reduce(i64 %dfredhandle{dest}, ptr {name_operand}, i32 {operation}, ptr %dfredout{dest}, ptr %dfredna{dest})"
    );
    let _ = writeln!(text, "  %dfrederr{dest} = icmp ne i32 %dfredrc{dest}, 0");
    let _ = writeln!(text, "  %dfredval{dest} = load double, ptr %dfredout{dest}");
    let _ = writeln!(
        text,
        "  %dfredmsg{dest} = select i1 %dfrederr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfredagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfrederr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfredagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfredagg0{dest}, ptr %dfredmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %dfredbits{dest} = bitcast double %dfredval{dest} to i64"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfredagg1{dest}, i64 %dfredbits{dest}, 2"
    );
}

pub(crate) fn lower_bndata_zscore(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let name = arguments[1];
    let name_operand = format!("%v{}", name.0);
    if llvm_type(
        analysis
            .values
            .get(&arguments[0])
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfzhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            arguments[0].0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfzhandle{dest} = ptrtoint ptr %v{} to i64",
            arguments[0].0
        );
    }
    let _ = writeln!(text, "  %dfzout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dfzrc{dest} = call i32 @bn_rt_dataframe_zscore(i64 %dfzhandle{dest}, ptr {name_operand}, ptr %dfzout{dest})"
    );
    let _ = writeln!(text, "  %dfzerr{dest} = icmp ne i32 %dfzrc{dest}, 0");
    let _ = writeln!(text, "  %dfzvalue{dest} = load i64, ptr %dfzout{dest}");
    let _ = writeln!(
        text,
        "  %dfzmsg{dest} = select i1 %dfzerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfzpayload{dest} = select i1 %dfzerr{dest}, i64 1, i64 %dfzvalue{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfzagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfzerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfzagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfzagg0{dest}, ptr %dfzmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfzagg1{dest}, i64 %dfzpayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_copy(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let length = match analysis.values.get(&arguments[2]) {
        Some(Type::Pointer {
            length: bn_types::PointerLength::Fixed(length),
            ..
        }) => *length,
        _ => 0,
    };
    if llvm_type(
        analysis
            .values
            .get(&arguments[0])
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfcopyhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            arguments[0].0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfcopyhandle{dest} = ptrtoint ptr %v{} to i64",
            arguments[0].0
        );
    }
    let target = if llvm_type(
        analysis
            .values
            .get(&arguments[2])
            .expect("validated copy target"),
    ) == Some("{ ptr, i32 }")
    {
        let _ = writeln!(
            text,
            "  %dfcopytarget{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
            arguments[2].0
        );
        let _ = writeln!(
            text,
            "  %dfcopylen{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
            arguments[2].0
        );
        format!("%dfcopytarget{dest}")
    } else {
        format!("%v{}", arguments[2].0)
    };
    let length = if length == 0 {
        format!("%dfcopylen{dest}")
    } else {
        length.to_string()
    };
    let _ = writeln!(
        text,
        "  %dfcopyrc{dest} = call i32 @{symbol}(i64 %dfcopyhandle{dest}, ptr %v{}, ptr {target}, i32 {length})",
        arguments[1].0
    );
    let _ = writeln!(text, "  %dfcopyerr{dest} = icmp ne i32 %dfcopyrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %dfcopymsg{dest} = select i1 %dfcopyerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfcopyagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfcopyerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfcopyagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfcopyagg0{dest}, ptr %dfcopymsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfcopyagg1{dest}, i64 0, 2"
    );
}

pub(crate) fn lower_bndata_select(text: &mut String, destination: ValueId, arguments: &[ValueId]) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %dfselhandle{dest} = ptrtoint ptr %v{} to i64",
        arguments[0].0
    );
    let _ = writeln!(
        text,
        "  %dfselrows{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        arguments[1].0
    );
    let _ = writeln!(
        text,
        "  %dfselrowlen{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
        arguments[1].0
    );
    let _ = writeln!(
        text,
        "  %dfselcols{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        arguments[2].0
    );
    let _ = writeln!(
        text,
        "  %dfselcollen{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
        arguments[2].0
    );
    let _ = writeln!(text, "  %dfselout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dfselrc{dest} = call i32 @bn_rt_dataframe_select(i64 %dfselhandle{dest}, ptr %dfselrows{dest}, i32 %dfselrowlen{dest}, ptr %dfselcols{dest}, i32 %dfselcollen{dest}, ptr %dfselout{dest})"
    );
    let _ = writeln!(text, "  %dfselerr{dest} = icmp ne i32 %dfselrc{dest}, 0");
    let _ = writeln!(text, "  %dfselvalue{dest} = load i64, ptr %dfselout{dest}");
    let _ = writeln!(
        text,
        "  %dfselmsg{dest} = select i1 %dfselerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfselpayload{dest} = select i1 %dfselerr{dest}, i64 1, i64 %dfselvalue{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfselagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfselerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfselagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfselagg0{dest}, ptr %dfselmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfselagg1{dest}, i64 %dfselpayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_transform(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %dftrhandle{dest} = ptrtoint ptr %v{} to i64",
        arguments[0].0
    );
    let _ = writeln!(text, "  %dftrout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dftrrc{dest} = call i32 @{symbol}(i64 %dftrhandle{dest}, ptr %dftrout{dest})"
    );
    let _ = writeln!(text, "  %dftrerr{dest} = icmp ne i32 %dftrrc{dest}, 0");
    let _ = writeln!(text, "  %dftrvalue{dest} = load i64, ptr %dftrout{dest}");
    let _ = writeln!(
        text,
        "  %dftrmsg{dest} = select i1 %dftrerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dftrpayload{dest} = select i1 %dftrerr{dest}, i64 1, i64 %dftrvalue{dest}"
    );
    let _ = writeln!(
        text,
        "  %dftragg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dftrerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dftragg1{dest} = insertvalue {{ i1, ptr, i64 }} %dftragg0{dest}, ptr %dftrmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dftragg1{dest}, i64 %dftrpayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_binary_transform(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %dfbinleft{dest} = ptrtoint ptr %v{} to i64",
        arguments[0].0
    );
    let _ = writeln!(
        text,
        "  %dfbinright{dest} = ptrtoint ptr %v{} to i64",
        arguments[1].0
    );
    let _ = writeln!(text, "  %dfbinout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dfbinrc{dest} = call i32 @{symbol}(i64 %dfbinleft{dest}, i64 %dfbinright{dest}, ptr %dfbinout{dest})"
    );
    let _ = writeln!(text, "  %dfbinerr{dest} = icmp ne i32 %dfbinrc{dest}, 0");
    let _ = writeln!(text, "  %dfbinvalue{dest} = load i64, ptr %dfbinout{dest}");
    let _ = writeln!(
        text,
        "  %dfbinmsg{dest} = select i1 %dfbinerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfbinpayload{dest} = select i1 %dfbinerr{dest}, i64 1, i64 %dfbinvalue{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfbinagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfbinerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfbinagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfbinagg0{dest}, ptr %dfbinmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfbinagg1{dest}, i64 %dfbinpayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_join(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    kind: u32,
) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %dfjoinleft{dest} = ptrtoint ptr %v{} to i64",
        arguments[0].0
    );
    let _ = writeln!(
        text,
        "  %dfjoinright{dest} = ptrtoint ptr %v{} to i64",
        arguments[1].0
    );
    let _ = writeln!(text, "  %dfjoinout{dest} = alloca i64");
    let _ = writeln!(
        text,
        "  %dfjoinrc{dest} = call i32 @bn_rt_dataframe_join(i64 %dfjoinleft{dest}, i64 %dfjoinright{dest}, ptr %v{}, ptr %v{}, i32 {kind}, ptr %dfjoinout{dest})",
        arguments[2].0, arguments[3].0
    );
    let _ = writeln!(text, "  %dfjoinerr{dest} = icmp ne i32 %dfjoinrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %dfjoinvalue{dest} = load i64, ptr %dfjoinout{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfjoinmsg{dest} = select i1 %dfjoinerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfjoinpayload{dest} = select i1 %dfjoinerr{dest}, i64 1, i64 %dfjoinvalue{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfjoinagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfjoinerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfjoinagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfjoinagg0{dest}, ptr %dfjoinmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfjoinagg1{dest}, i64 %dfjoinpayload{dest}, 2"
    );
}
