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
    emit_void_result(text, destination, format!("%dflabelrc{dest}"));
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
    let _ = writeln!(text, "  %dfredval{dest} = load double, ptr %dfredout{dest}");
    let _ = writeln!(
        text,
        "  %dfredbits{dest} = bitcast double %dfredval{dest} to i64"
    );
    emit_status_result(
        text,
        destination,
        &format!("%dfredrc{dest}"),
        None,
        "null",
        &format!("%dfredbits{dest}"),
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
    let _ = writeln!(text, "  %dfzvalue{dest} = load i64, ptr %dfzout{dest}");
    emit_handle_result(
        text,
        destination,
        format!("%dfzrc{dest}"),
        format!("%dfzvalue{dest}"),
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
    emit_void_result(text, destination, format!("%dfcopyrc{dest}"));
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
    let _ = writeln!(text, "  %dfselvalue{dest} = load i64, ptr %dfselout{dest}");
    emit_handle_result(
        text,
        destination,
        format!("%dfselrc{dest}"),
        format!("%dfselvalue{dest}"),
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
    let _ = writeln!(text, "  %dftrvalue{dest} = load i64, ptr %dftrout{dest}");
    emit_handle_result(
        text,
        destination,
        format!("%dftrrc{dest}"),
        format!("%dftrvalue{dest}"),
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
    let _ = writeln!(text, "  %dfbinvalue{dest} = load i64, ptr %dfbinout{dest}");
    emit_handle_result(
        text,
        destination,
        format!("%dfbinrc{dest}"),
        format!("%dfbinvalue{dest}"),
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
    let _ = writeln!(
        text,
        "  %dfjoinvalue{dest} = load i64, ptr %dfjoinout{dest}"
    );
    emit_handle_result(
        text,
        destination,
        format!("%dfjoinrc{dest}"),
        format!("%dfjoinvalue{dest}"),
    );
}
