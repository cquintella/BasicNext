#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_bndata_count(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let receiver = arguments.first().expect("validated DataFrame receiver");
    let continuation = take_continuation(block_id, state);
    let handle = format!("%dfcount_handle{}", destination.0);
    let slot = format!("dfcount_out{}", destination.0);
    if llvm_type(
        analysis
            .values
            .get(receiver)
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  {handle} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            receiver.0
        );
    } else {
        let _ = writeln!(text, "  {handle} = ptrtoint ptr %v{} to i64", receiver.0);
    }
    let _ = writeln!(text, "  %{slot} = alloca i32");
    let _ = writeln!(
        text,
        "  %dfcount_rc{} = call i32 @{symbol}(i64 {handle}, ptr %{slot})",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %dfcount_ok{} = icmp eq i32 %dfcount_rc{}, 0",
        destination.0, destination.0
    );
    let _ = writeln!(
        text,
        "  br i1 %dfcount_ok{}, label %{continuation}, label %trap_bn_rt",
        destination.0
    );
    state.control_flow.label(text, continuation.clone());
    let _ = writeln!(text, "  %v{} = load i32, ptr %{slot}", destination.0);
    state.needs_bn_rt_trap = true;
}

pub(crate) fn lower_bndata_add_integer_column(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let receiver = arguments[0];
    let name = arguments[1];
    let vector = arguments[2];
    let Type::Vector { dimensions, .. } = analysis.values.get(&vector).expect("validated vector")
    else {
        unreachable!("validated integer vector");
    };
    let length = dimensions[0];
    let _ = writeln!(
        text,
        "  %dfaddhandle{dest} = ptrtoint ptr %v{} to i64",
        receiver.0
    );
    let _ = writeln!(
        text,
        "  %dfadddata{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        vector.0
    );
    let _ = writeln!(
        text,
        "  %dfaddcolumn{dest} = call i32 @bn_rt_dataframe_add_integer_start(i64 %dfaddhandle{dest}, ptr %v{}, i32 {length})",
        name.0
    );
    let _ = writeln!(
        text,
        "  %dfadderror0_{dest} = icmp slt i32 %dfaddcolumn{dest}, 0"
    );
    let mut previous = format!("%dfadderror0_{dest}");
    for index in 0..length {
        let _ = writeln!(
            text,
            "  %dfaddptr{dest}_{index} = getelementptr i32, ptr %dfadddata{dest}, i64 {index}"
        );
        let _ = writeln!(
            text,
            "  %dfaddvalue{dest}_{index} = load i32, ptr %dfaddptr{dest}_{index}"
        );
        let _ = writeln!(
            text,
            "  %dfaddvalue64_{dest}_{index} = sext i32 %dfaddvalue{dest}_{index} to i64"
        );
        let _ = writeln!(
            text,
            "  %dfaddrc{dest}_{index} = call i32 @bn_rt_dataframe_set_integer_cell(i64 %dfaddhandle{dest}, i32 %dfaddcolumn{dest}, i32 {index}, i64 %dfaddvalue64_{dest}_{index})"
        );
        let _ = writeln!(
            text,
            "  %dfaddbad{dest}_{index} = icmp ne i32 %dfaddrc{dest}_{index}, 0"
        );
        let _ = writeln!(
            text,
            "  %dfadderror{}_{dest} = or i1 {previous}, %dfaddbad{dest}_{index}",
            index + 1
        );
        previous = format!("%dfadderror{}_{dest}", index + 1);
    }
    let _ = writeln!(
        text,
        "  %dfaddduplicate{dest} = icmp eq i32 %dfaddcolumn{dest}, -4"
    );
    let _ = writeln!(
        text,
        "  %dfaddlength{dest} = icmp eq i32 %dfaddcolumn{dest}, -5"
    );
    let _ = writeln!(
        text,
        "  %dfaddmessage0_{dest} = select i1 %dfaddduplicate{dest}, ptr @.bn_dataframe_duplicate, ptr @.bn_dataframe_error"
    );
    let _ = writeln!(
        text,
        "  %dfaddmessage1_{dest} = select i1 %dfaddlength{dest}, ptr @.bn_dataframe_length, ptr %dfaddmessage0_{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfaddmessage{dest} = select i1 {previous}, ptr %dfaddmessage1_{dest}, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfaddpayload{dest} = select i1 {previous}, i64 1, i64 0"
    );
    let _ = writeln!(
        text,
        "  %dfaddagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 {previous}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfaddagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfaddagg0{dest}, ptr %dfaddmessage{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfaddagg1{dest}, i64 %dfaddpayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_add_simple_column(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let receiver = arguments[0];
    let name = arguments[1];
    let vector = arguments[2];
    let Type::Vector { dimensions, .. } = analysis.values.get(&vector).expect("validated vector")
    else {
        unreachable!("validated DataFrame column vector");
    };
    let length = dimensions[0];
    let _ = writeln!(
        text,
        "  %dfsimplehandle{dest} = ptrtoint ptr %v{} to i64",
        receiver.0
    );
    let _ = writeln!(
        text,
        "  %dfsimpledata{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        vector.0
    );
    let _ = writeln!(
        text,
        "  %dfsimplerc{dest} = call i32 @{symbol}(i64 %dfsimplehandle{dest}, ptr %v{}, ptr %dfsimpledata{dest}, i32 {length})",
        name.0
    );
    let _ = writeln!(
        text,
        "  %dfsimpleerr{dest} = icmp ne i32 %dfsimplerc{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfsimplemsg{dest} = select i1 %dfsimpleerr{dest}, ptr @.bn_dataframe_error, ptr null"
    );
    let _ = writeln!(
        text,
        "  %dfsimplepayload{dest} = select i1 %dfsimpleerr{dest}, i64 1, i64 0"
    );
    let _ = writeln!(
        text,
        "  %dfsimpleagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfsimpleerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfsimpleagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfsimpleagg0{dest}, ptr %dfsimplemsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfsimpleagg1{dest}, i64 %dfsimplepayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_column_name(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let index_value = arguments[1];
    let index = coerce_to_type(
        text,
        index_value,
        analysis
            .values
            .get(&index_value)
            .expect("validated column index"),
        &Type::Integer(IntegerType::Int32),
    );
    if llvm_type(
        analysis
            .values
            .get(&arguments[0])
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfnamehandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            arguments[0].0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfnamehandle{dest} = ptrtoint ptr %v{} to i64",
            arguments[0].0
        );
    }
    let _ = writeln!(
        text,
        "  %dfnameptr{dest} = call ptr @bn_rt_dataframe_column_name_owned(i64 %dfnamehandle{dest}, i32 {index})"
    );
    let _ = writeln!(
        text,
        "  %dfnameerror{dest} = icmp eq ptr %dfnameptr{dest}, null"
    );
    let _ = writeln!(
        text,
        "  %dfnamemessage{dest} = select i1 %dfnameerror{dest}, ptr @.bn_dataframe_index, ptr %dfnameptr{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfnamepayload{dest} = select i1 %dfnameerror{dest}, i64 1, i64 0"
    );
    let _ = writeln!(
        text,
        "  %dfnameagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfnameerror{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfnameagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfnameagg0{dest}, ptr %dfnamemessage{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfnameagg1{dest}, i64 %dfnamepayload{dest}, 2"
    );
}

pub(crate) fn lower_bndata_status_call(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let receiver = arguments[0];
    let first = coerce_to_type(
        text,
        arguments[1],
        analysis
            .values
            .get(&arguments[1])
            .expect("validated DataFrame argument"),
        &Type::Integer(IntegerType::Int32),
    );
    let name = arguments[2];
    let name_ty = analysis
        .values
        .get(&name)
        .expect("validated DataFrame column name");
    let name_operand = if llvm_type(name_ty) == Some("{ i1, ptr, i64 }") {
        let _ = writeln!(
            text,
            "  %dfgetname{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            name.0
        );
        format!("%dfgetname{dest}")
    } else {
        format!("%v{}", name.0)
    };
    if llvm_type(
        analysis
            .values
            .get(&receiver)
            .expect("validated DataFrame receiver"),
    ) == Some("{ i1, ptr, i64 }")
    {
        let _ = writeln!(
            text,
            "  %dfgethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
            receiver.0
        );
    } else {
        let _ = writeln!(
            text,
            "  %dfgethandle{dest} = ptrtoint ptr %v{} to i64",
            receiver.0
        );
    }
    let _ = writeln!(text, "  %dfgetout{dest} = alloca i64");
    let _ = writeln!(text, "  %dfgetna{dest} = alloca i8");
    let _ = writeln!(text, "  store i64 0, ptr %dfgetout{dest}");
    let _ = writeln!(text, "  store i8 0, ptr %dfgetna{dest}");
    let _ = writeln!(
        text,
        "  %dfgetrc{dest} = call i32 @{symbol}(i64 %dfgethandle{dest}, i32 {first}, ptr {name_operand}, ptr %dfgetout{dest}, ptr %dfgetna{dest})"
    );
    let _ = writeln!(text, "  %dfgeterr{dest} = icmp ne i32 %dfgetrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %dfgetpayload{dest} = load i64, ptr %dfgetout{dest}"
    );
    let _ = writeln!(text, "  %dfgetnatag{dest} = load i8, ptr %dfgetna{dest}");
    let _ = writeln!(text, "  %dfgetisna{dest} = icmp ne i8 %dfgetnatag{dest}, 0");
    let value_pointer = if symbol == "bn_rt_dataframe_get_string" {
        let _ = writeln!(
            text,
            "  %dfgetstring{dest} = inttoptr i64 %dfgetpayload{dest} to ptr"
        );
        format!("%dfgetstring{dest}")
    } else {
        "null".into()
    };
    let _ = writeln!(
        text,
        "  %dfgetnaptr{dest} = getelementptr [3 x i8], ptr @.bn_na, i64 0, i64 0"
    );
    let _ = writeln!(
        text,
        "  %dfgetvalueptr{dest} = select i1 %dfgetisna{dest}, ptr %dfgetnaptr{dest}, ptr {value_pointer}"
    );
    let _ = writeln!(
        text,
        "  %dfgetmsg{dest} = select i1 %dfgeterr{dest}, ptr @.bn_dataframe_error, ptr %dfgetvalueptr{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfgetcode{dest} = select i1 %dfgeterr{dest}, i64 1, i64 %dfgetpayload{dest}"
    );
    let _ = writeln!(
        text,
        "  %dfgetagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dfgeterr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %dfgetagg1{dest} = insertvalue {{ i1, ptr, i64 }} %dfgetagg0{dest}, ptr %dfgetmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %dfgetagg1{dest}, i64 %dfgetcode{dest}, 2"
    );
}
