#![allow(clippy::wildcard_imports)]
use super::runtime::is_bndata_function;
use super::*;
use crate::functions::dispatch_trampoline_symbol;

fn lower_for_condition(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let current = arguments[0];
    let end = arguments[1];
    let step = arguments[2];
    let current_type = analysis
        .values
        .get(&current)
        .expect("validated FOR current type");
    let step_type = analysis.values.get(&step).expect("validated FOR step type");
    let current_i64 = extend_to_i64(text, current, current_type);
    let end_i64 = extend_to_i64(
        text,
        end,
        analysis.values.get(&end).expect("validated FOR end type"),
    );
    let positive_opcode = integer_compare_opcode("Greater", step_type);
    let ascending_opcode = if is_unsigned(current_type) {
        "icmp ule"
    } else {
        "icmp sle"
    };
    let descending_opcode = if is_unsigned(current_type) {
        "icmp uge"
    } else {
        "icmp sge"
    };
    let step_llvm_ty = llvm_type(step_type).expect("validated FOR step type");
    let _ = writeln!(
        text,
        "  %for_step_positive{} = {positive_opcode} {step_llvm_ty} %v{}, 0",
        destination.0, step.0
    );
    let _ = writeln!(
        text,
        "  %for_ascending{} = {ascending_opcode} i64 {current_i64}, {end_i64}",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %for_descending{} = {descending_opcode} i64 {current_i64}, {end_i64}",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %v{} = select i1 %for_step_positive{}, i1 %for_ascending{}, i1 %for_descending{}",
        destination.0, destination.0, destination.0, destination.0
    );
}

fn lower_bndata_count(
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

fn lower_bndata_add_integer_column(
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

fn lower_bndata_add_simple_column(
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

fn lower_bndata_column_name(
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

fn lower_bndata_status_call(
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

fn lower_bndata_set_label(text: &mut String, destination: ValueId, arguments: &[ValueId]) {
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

fn lower_bndata_reduce(
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

fn lower_bndata_zscore(
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

fn lower_bndata_copy(
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

fn lower_bndata_select(text: &mut String, destination: ValueId, arguments: &[ValueId]) {
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

fn lower_bndata_transform(
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

fn lower_bndata_binary_transform(
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

fn lower_bndata_join(text: &mut String, destination: ValueId, arguments: &[ValueId], kind: u32) {
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

fn lower_bndata_convert(
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

fn lower_bndata_slice(
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

fn lower_bnlog_status(text: &mut String, destination: ValueId, call: &str) {
    let dest = destination.0;
    let _ = writeln!(text, "  %logrc{dest} = {call}");
    let _ = writeln!(text, "  %logerr{dest} = icmp ne i32 %logrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %logmsg{dest} = select i1 %logerr{dest}, ptr @.bn_log_error, ptr null"
    );
    let _ = writeln!(text, "  %logcode{dest} = sext i32 %logrc{dest} to i64");
    let _ = writeln!(
        text,
        "  %logagg0_{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %logerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %logagg1_{dest} = insertvalue {{ i1, ptr, i64 }} %logagg0_{dest}, ptr %logmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %logagg1_{dest}, i64 %logcode{dest}, 2"
    );
}

fn lower_bnlog_call(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    if matches!(method, "fields_constructor" | "logger_constructor") {
        return;
    }
    let handle = |text: &mut String, tag: &str, value: ValueId| {
        let name = format!("%logarg{}_{}_handle", destination.0, tag);
        let _ = writeln!(text, "  {name} = ptrtoint ptr %v{} to i64", value.0);
        name
    };
    let integer = |text: &mut String, value: ValueId| {
        extend_to_i64(
            text,
            value,
            analysis
                .values
                .get(&value)
                .expect("validated BNLog integer"),
        )
    };
    let call = match method {
        "fields_set_string" => {
            let receiver = handle(text, "receiver", arguments[0]);
            format!(
                "call i32 @bn_rt_log_fields_set_string(i64 {receiver}, ptr %v{}, ptr %v{})",
                arguments[1].0, arguments[2].0
            )
        }
        "logger_add_file" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let minimum = integer(text, arguments[2]);
            format!(
                "call i32 @bn_rt_log_logger_add_file(i64 {receiver}, ptr %v{}, i64 {minimum})",
                arguments[1].0
            )
        }
        "logger_log" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let fields = handle(text, "fields", arguments[3]);
            let level = integer(text, arguments[1]);
            format!(
                "call i32 @bn_rt_log_logger_log(i64 {receiver}, i64 {level}, ptr %v{}, i64 {fields})",
                arguments[2].0
            )
        }
        "logger_flush" | "logger_close" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let timeout = integer(text, arguments[1]);
            let symbol = if method == "logger_flush" {
                "bn_rt_log_logger_flush"
            } else {
                "bn_rt_log_logger_close"
            };
            format!("call i32 @{symbol}(i64 {receiver}, i64 {timeout})")
        }
        _ => unreachable!("validated BNLog method"),
    };
    lower_bnlog_status(text, destination, &call);
}

fn lower_indirect_call(
    text: &mut String,
    destination: ValueId,
    callee: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let Type::Function {
        parameters,
        return_type,
    } = analysis
        .values
        .get(&callee)
        .expect("validated indirect callee")
    else {
        unreachable!("validated indirect function type");
    };
    let operands = arguments
        .iter()
        .zip(parameters)
        .map(|(argument, parameter)| {
            let llvm_ty = llvm_type(parameter).expect("validated indirect parameter");
            let operand = coerce_to_type(
                text,
                *argument,
                analysis.values.get(argument).expect("validated argument"),
                parameter,
            );
            format!("{llvm_ty} {operand}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    if is_void_type(return_type) {
        emit_void_result(
            text,
            destination,
            format!("call void %v{}({operands})", callee.0),
        );
    } else {
        let return_llvm = llvm_type(return_type).expect("validated indirect return");
        let _ = writeln!(
            text,
            "  %v{} = call {return_llvm} %v{}({operands})",
            destination.0, callee.0
        );
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower_scalar_instruction_tail(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    match instruction {
        Instruction::Call {
            destination,
            callee,
            arguments,
            ..
        } => {
            if !analysis.functions.contains_key(callee)
                && matches!(analysis.values.get(callee), Some(Type::Function { .. }))
            {
                lower_indirect_call(text, *destination, *callee, arguments, analysis);
                return Ok(());
            }
            match analysis
                .functions
                .get(callee)
                .copied()
                .expect("validated callee")
            {
                "$for_condition" => lower_for_condition(text, *destination, arguments, analysis),
                name if is_bndata_dataframe_call(module, name) => {
                    match bndata_dataframe_method(name).expect("validated BNData method") {
                        "constructor" => {}
                        "row_count" | "column_count" => lower_bndata_count(
                            text,
                            block_id,
                            *destination,
                            arguments,
                            if name.ends_with("RowCount") {
                                "bn_rt_dataframe_row_count"
                            } else {
                                "bn_rt_dataframe_column_count"
                            },
                            analysis,
                            state,
                        ),
                        "add_integer_column" => {
                            lower_bndata_add_integer_column(
                                text,
                                *destination,
                                arguments,
                                analysis,
                            );
                        }
                        "add_string_column" => lower_bndata_add_simple_column(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_add_string",
                        ),
                        "add_float_column" => lower_bndata_add_simple_column(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_add_float",
                        ),
                        "add_boolean_column" => lower_bndata_add_simple_column(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_add_boolean",
                        ),
                        "column_name" => {
                            lower_bndata_column_name(text, *destination, arguments, analysis);
                        }
                        "set_label" => lower_bndata_set_label(text, *destination, arguments),
                        "get_string" => lower_bndata_status_call(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_get_string",
                        ),
                        "get_integer" => lower_bndata_status_call(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_get_integer",
                        ),
                        "get_float" => lower_bndata_status_call(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_get_float",
                        ),
                        "get_boolean" => lower_bndata_status_call(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_get_boolean",
                        ),
                        "mean" | "median" | "quartile1" | "quartile3" | "mode" | "stdev"
                        | "variance" | "range" | "min" | "max" => {
                            let operation =
                                match bndata_dataframe_method(name).expect("validated reduction") {
                                    "mean" => 0,
                                    "median" => 1,
                                    "quartile1" => 2,
                                    "quartile3" => 3,
                                    "mode" => 4,
                                    "stdev" => 5,
                                    "variance" => 6,
                                    "range" => 7,
                                    "min" => 8,
                                    "max" => 9,
                                    _ => unreachable!(),
                                };
                            lower_bndata_reduce(text, *destination, arguments, operation, analysis);
                        }
                        "zscore" => lower_bndata_zscore(text, *destination, arguments, analysis),
                        "copy_integer" => lower_bndata_copy(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_copy_integer",
                        ),
                        "copy_float" => lower_bndata_copy(
                            text,
                            *destination,
                            arguments,
                            analysis,
                            "bn_rt_dataframe_copy_float",
                        ),
                        "select" => lower_bndata_select(text, *destination, arguments),
                        "slice" => lower_bndata_slice(text, *destination, arguments, analysis),
                        "transpose" => lower_bndata_transform(
                            text,
                            *destination,
                            arguments,
                            "bn_rt_dataframe_transpose",
                        ),
                        "append_rows" => lower_bndata_binary_transform(
                            text,
                            *destination,
                            arguments,
                            "bn_rt_dataframe_append_rows",
                        ),
                        "append_columns" => lower_bndata_binary_transform(
                            text,
                            *destination,
                            arguments,
                            "bn_rt_dataframe_append_columns",
                        ),
                        "join" => lower_bndata_join(text, *destination, arguments, 0),
                        "left_join" => lower_bndata_join(text, *destination, arguments, 1),
                        "right_join" => lower_bndata_join(text, *destination, arguments, 2),
                        "full_join" => lower_bndata_join(text, *destination, arguments, 3),
                        "convert_integer" => lower_bndata_convert(
                            text,
                            *destination,
                            arguments,
                            "bn_rt_dataframe_convert_integer",
                            analysis,
                        ),
                        "convert_float" => lower_bndata_convert(
                            text,
                            *destination,
                            arguments,
                            "bn_rt_dataframe_convert_float",
                            analysis,
                        ),
                        _ => unreachable!("validated BNData DataFrame method"),
                    }
                }
                name if is_bndata_function(name) => {
                    let dest = destination.0;
                    if name.ends_with("WriteCSV") {
                        let _ = writeln!(
                            text,
                            "  %csvfilew{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                            arguments[0].0
                        );
                        if llvm_type(
                            analysis
                                .values
                                .get(&arguments[1])
                                .expect("validated DataFrame argument"),
                        ) == Some("{ i1, ptr, i64 }")
                        {
                            let _ = writeln!(
                                text,
                                "  %csvframew{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                                arguments[1].0
                            );
                        } else {
                            let _ = writeln!(
                                text,
                                "  %csvframew{dest} = ptrtoint ptr %v{} to i64",
                                arguments[1].0
                            );
                        }
                        let _ = writeln!(
                            text,
                            "  %csvheaderw{dest} = zext i1 %v{} to i8",
                            arguments[2].0
                        );
                        emit_void_result(
                            text,
                            *destination,
                            format!(
                                "call i32 @bn_rt_dataframe_write_csv(i64 %csvfilew{dest}, i64 %csvframew{dest}, i8 %csvheaderw{dest}, ptr %v{})",
                                arguments[3].0
                            ),
                        );
                        return Ok(());
                    }
                    let _ = writeln!(
                        text,
                        "  %csvfile{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                        arguments[0].0
                    );
                    let _ = writeln!(
                        text,
                        "  %csvheader{dest} = zext i1 %v{} to i8",
                        arguments[1].0
                    );
                    let _ = writeln!(text, "  %csvout{dest} = alloca i64");
                    let _ = writeln!(
                        text,
                        "  %csvrc{dest} = call i32 @bn_rt_dataframe_read_csv(i64 %csvfile{dest}, i8 %csvheader{dest}, ptr %v{}, ptr %csvout{dest})",
                        arguments[2].0
                    );
                    let _ = writeln!(text, "  %csverr{dest} = icmp ne i32 %csvrc{dest}, 0");
                    let _ = writeln!(text, "  %csvvalue{dest} = load i64, ptr %csvout{dest}");
                    let _ = writeln!(
                        text,
                        "  %csvmsg{dest} = select i1 %csverr{dest}, ptr @.bn_dataframe_error, ptr null"
                    );
                    let _ = writeln!(
                        text,
                        "  %csvpayload{dest} = select i1 %csverr{dest}, i64 1, i64 %csvvalue{dest}"
                    );
                    let _ = writeln!(
                        text,
                        "  %csvagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %csverr{dest}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %csvagg1{dest} = insertvalue {{ i1, ptr, i64 }} %csvagg0{dest}, ptr %csvmsg{dest}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %csvagg1{dest}, i64 %csvpayload{dest}, 2"
                    );
                }
                name if bnlog_method(module, name).is_some() => lower_bnlog_call(
                    text,
                    *destination,
                    bnlog_method(module, name).expect("validated BNLog method"),
                    arguments,
                    analysis,
                ),
                name if bnmath_method(module, name).is_some() => {
                    lower_bnmath_call(
                        text,
                        *destination,
                        bnmath_method(module, name).expect("validated BNMath"),
                        arguments,
                        analysis,
                    );
                }
                name if is_bn_rt_host_call(name) => {
                    lower_bn_rt_call(
                        text,
                        block_id,
                        *destination,
                        name,
                        arguments,
                        analysis,
                        state,
                    );
                }
                name if name.ends_with(".Queue.Concurrent")
                    || name.ends_with(".Queue.Serial")
                    || name.ends_with(".Queue.Auto")
                    || name.ends_with(".Queue.Join")
                    || name.ends_with(".Queue.Close")
                    || name.ends_with(".Ticket.Close")
                    || name.ends_with(".Group.New")
                    || name.ends_with(".Group.Enter")
                    || name.ends_with(".Group.Leave")
                    || name.ends_with(".Group.Wait")
                    || name.ends_with(".Barrier.New")
                    || name.ends_with(".Barrier.Wait")
                    || name.ends_with(".Semaphore.New")
                    || name.ends_with(".Semaphore.Acquire")
                    || name.ends_with(".Semaphore.Release")
                    || name.ends_with(".Mutex.New")
                    || name.ends_with(".Mutex.Lock")
                    || name.ends_with(".Mutex.Unlock") =>
                {
                    lower_bn_dispatch_call(text, *destination, name, arguments, analysis);
                }
                "TimeZone.Parse" => {
                    let argument = arguments[0];
                    let _ = writeln!(
                        text,
                        "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                        destination.0, argument.0
                    );
                }
                "ASC" => {
                    let dest = destination.0;
                    let argument = arguments[0];
                    let _ = writeln!(
                        text,
                        "  %asccode{dest} = call i64 @bn_rt_str_asc(ptr %v{})",
                        argument.0
                    );
                    let _ = writeln!(text, "  %ascerror{dest} = icmp slt i64 %asccode{dest}, 0");
                    let _ = writeln!(
                        text,
                        "  %ascmessage{dest} = select i1 %ascerror{dest}, ptr @.bn_asc_error, ptr null"
                    );
                    let _ = writeln!(
                        text,
                        "  %ascpayload{dest} = select i1 %ascerror{dest}, i64 1, i64 %asccode{dest}"
                    );
                    let _ = writeln!(
                        text,
                        "  %ascagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %ascerror{dest}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %ascagg1{dest} = insertvalue {{ i1, ptr, i64 }} %ascagg0{dest}, ptr %ascmessage{dest}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %ascagg1{dest}, i64 %ascpayload{dest}, 2"
                    );
                }
                "CHAR" => {
                    let dest = destination.0;
                    let argument = arguments[0];
                    let code = extend_to_i64(
                        text,
                        argument,
                        analysis
                            .values
                            .get(&argument)
                            .expect("validated CHAR argument"),
                    );
                    let _ = writeln!(
                        text,
                        "  %charpacked{dest} = call i64 @bn_rt_str_char_utf8(i64 {code})"
                    );
                    let _ = writeln!(
                        text,
                        "  %charerror{dest} = icmp eq i64 %charpacked{dest}, -1"
                    );
                    let _ = writeln!(text, "  %charbuffer{dest} = alloca i64");
                    let _ = writeln!(text, "  store i64 %charpacked{dest}, ptr %charbuffer{dest}");
                    let _ = writeln!(
                        text,
                        "  %charvalue{dest} = select i1 %charerror{dest}, ptr @.bn_char_error, ptr %charbuffer{dest}"
                    );
                    let _ = writeln!(
                        text,
                        "  %charpayload{dest} = select i1 %charerror{dest}, i64 1, i64 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %charagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %charerror{dest}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %charagg1{dest} = insertvalue {{ i1, ptr, i64 }} %charagg0{dest}, ptr %charvalue{dest}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %charagg1{dest}, i64 %charpayload{dest}, 2"
                    );
                }
                name if module.functions.iter().any(|function| {
                    function.name == name.strip_prefix("@super:").unwrap_or(name)
                }) =>
                {
                    lower_user_call(text, module, *destination, name, arguments, analysis, state);
                }
                "HOST.Random.Seed" => {
                    let seed = extend_to_i64(
                        text,
                        *arguments.first().expect("validated seed argument"),
                        analysis
                            .values
                            .get(arguments.first().expect("validated seed argument"))
                            .expect("validated seed type"),
                    );
                    emit_checked_i32_eq_zero(
                        text,
                        block_id,
                        *destination,
                        &format!("call i32 @bn_rt_random_seed(i64 {seed})"),
                        state,
                    );
                }
                "HOST.Random.Random" => {
                    let _ = writeln!(
                        text,
                        "  %v{} = call double @bn_rt_random_next()",
                        destination.0
                    );
                }
                _ => unreachable!("validated call target"),
            }
        }
        Instruction::DispatchSubmit {
            destination,
            queue,
            task,
            ..
        } => {
            let task_name = analysis
                .functions
                .get(task)
                .copied()
                .expect("validated async task target");
            let _ = writeln!(
                text,
                "  %dispatchqueue{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, queue.0
            );
            let _ = writeln!(text, "  %dispatchticket{} = alloca i64", destination.0);
            let _ = writeln!(
                text,
                "  %dispatchrc{} = call i32 @bn_rt_dispatch_submit(i64 %dispatchqueue{}, ptr @{}, ptr null, ptr null, i32 0, ptr %dispatchticket{})",
                destination.0,
                destination.0,
                dispatch_trampoline_symbol(task_name),
                destination.0
            );
            let _ = writeln!(
                text,
                "  %dispatchhandle{} = load i64, ptr %dispatchticket{}",
                destination.0, destination.0
            );
            emit_handle_result(
                text,
                *destination,
                format!("%dispatchrc{}", destination.0),
                format!("%dispatchhandle{}", destination.0),
            );
        }
        Instruction::DispatchAwait {
            destination,
            ticket,
            timeout,
            ..
        } => {
            let _ = writeln!(
                text,
                "  %dispatchticket{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, ticket.0
            );
            let timeout_ty = analysis
                .values
                .get(timeout)
                .expect("validated timeout type");
            let timeout = extend_to_i64(text, *timeout, timeout_ty);
            let _ = writeln!(
                text,
                "  %dispatchresult{} = alloca [32 x i8]",
                destination.0
            );
            let _ = writeln!(text, "  %dispatcherror{} = alloca [24 x i8]", destination.0);
            emit_void_result(
                text,
                *destination,
                format!(
                    "call i32 @bn_rt_dispatch_await(i64 %dispatchticket{}, i64 {}, ptr %dispatchresult{}, ptr %dispatcherror{})",
                    destination.0, timeout, destination.0, destination.0
                ),
            );
        }
        Instruction::Input {
            destination,
            prompt,
            ..
        } => {
            block_state.constants.remove(destination);
            if let Some(prompt) = prompt {
                let _ = writeln!(
                    text,
                    "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %v{})",
                    prompt.0
                );
            }
            let symbol = analysis
                .input_targets
                .get(destination)
                .expect("validated INPUT owner");
            let slot = symbols[symbol];
            let dest = destination.0;
            let _ = writeln!(text, "  %inputold{dest} = load ptr, ptr %s{slot}");
            let _ = writeln!(
                text,
                "  %inputwasowned{dest} = load i1, ptr %inputowned{slot}"
            );
            let _ = writeln!(
                text,
                "  %inputreuse{dest} = select i1 %inputwasowned{dest}, ptr %inputold{dest}, ptr null"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = call ptr @bn_input(ptr %inputreuse{dest})"
            );
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let _ = writeln!(text, "  %v{} = add i32 0, %argc", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if analysis.values.get(vector) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let _ = writeln!(
                text,
                "  %v{} = call i32 @bn_rt_str_len(ptr %v{})",
                destination.0, vector.0
            );
        }
        Instruction::SizeOf {
            destination, value, ..
        } => {
            block_state.constants.remove(destination);
            let continuation = take_continuation(block_id, state);
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %sizeofbytes{dest} = call i64 @bn_string_byte_length(ptr %v{})",
                value.0
            );
            let _ = writeln!(
                text,
                "  %sizeofok{dest} = icmp ule i64 %sizeofbytes{dest}, 2147483647"
            );
            let _ = writeln!(
                text,
                "  br i1 %sizeofok{dest}, label %{continuation}, label %trap_numeric_overflow"
            );
            state.control_flow.label(text, continuation);
            let _ = writeln!(text, "  %v{dest} = trunc i64 %sizeofbytes{dest} to i32");
            state.needs_numeric_overflow_trap = true;
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if matches!(
            analysis.values.get(vector),
            Some(Type::Vector { .. } | Type::Pointer { .. })
        ) =>
        {
            block_state.constants.remove(destination);
            emit_vector_length(text, *destination, *vector);
        }
        Instruction::Vector {
            destination,
            values: elements,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            emit_vector(text, module, *destination, elements, ty, analysis);
        }
        Instruction::Allocate {
            destination,
            type_name,
            arguments,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let object_bytes = class_instance_bytes(module, type_name);
            emit_allocate(
                text,
                module,
                *destination,
                arguments,
                ty,
                analysis,
                object_bytes,
            );
            if !matches!(ty, Type::Pointer { .. })
                && !is_bndata_dataframe_type(module, ty)
                && bnlog_resource_kind(module, ty).is_none()
            {
                let class_global = format!("@.bn_cls_{}", sanitize_symbol(type_name));
                emit_store_object_class(text, *destination, &class_global);
            }
        }
        Instruction::Delete {
            value, destructor, ..
        } => {
            let ty = analysis
                .values
                .get(value)
                .expect("validated delete value type");
            if is_bndata_dataframe_type(module, ty) {
                let handle = format!("%dfdelhandle{}", value.0);
                let _ = writeln!(text, "  {handle} = ptrtoint ptr %v{} to i64", value.0);
                emit_checked_i32_eq_zero(
                    text,
                    block_id,
                    *value,
                    &format!("call i32 @bn_rt_dataframe_close(i64 {handle})"),
                    state,
                );
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }")
                && matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(|item| matches!(item, Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } if name == "DataFrame")))
            {
                let tag = format!("dfaltdelete{}", value.0);
                let continuation = take_continuation(block_id, state);
                let _ = writeln!(
                    text,
                    "  %dfaltiserr{} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
                    value.0, value.0
                );
                let _ = writeln!(
                    text,
                    "  br i1 %dfaltiserr{}, label %{}, label %{}",
                    value.0, continuation, tag
                );
                state.control_flow.label(text, tag.clone());
                let _ = writeln!(
                    text,
                    "  %dfalthandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    value.0, value.0
                );
                emit_checked_i32_eq_zero(
                    text,
                    block_id,
                    *value,
                    &format!(
                        "call i32 @bn_rt_dataframe_close(i64 %dfalthandle{})",
                        value.0
                    ),
                    state,
                );
                let _ = writeln!(text, "  br label %{continuation}");
                state.control_flow.label(text, continuation);
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }") {
                let _ = writeln!(
                    text,
                    "  %filedelhandle{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    value.0, value.0
                );
                let _ = writeln!(
                    text,
                    "  call i32 @bn_rt_file_close(i64 %filedelhandle{})",
                    value.0
                );
            } else if let Some(kind) = bnlog_resource_kind(module, ty) {
                let handle = format!("%logdelhandle{}", value.0);
                let symbol = if kind == "Fields" {
                    "bn_rt_log_fields_close"
                } else {
                    "bn_rt_log_logger_delete"
                };
                let _ = writeln!(text, "  {handle} = ptrtoint ptr %v{} to i64", value.0);
                let _ = writeln!(
                    text,
                    "  %logdelrc{} = call i32 @{symbol}(i64 {handle})",
                    value.0
                );
            } else {
                if let Some(destructor) = destructor {
                    let symbol = llvm_function_symbol(destructor);
                    let _ = writeln!(text, "  call void @{symbol}(ptr %v{})", value.0);
                }
                emit_delete(text, module, *value, ty);
            }
        }
        Instruction::EnsureClass { class, .. } => {
            let flag = class_init_flag(class);
            let init_name = format!("{class}.$init");
            let n = state.continuation_count;
            state.continuation_count += 1;
            let tag = format!("{}{n}", sanitize_symbol(class));
            let _ = writeln!(text, "  %initflag{tag} = load i1, ptr {flag}");
            let _ = writeln!(
                text,
                "  br i1 %initflag{tag}, label %initdone{tag}, label %initrun{tag}"
            );
            state.control_flow.label(text, format!("initrun{tag}"));
            let _ = writeln!(text, "  store i1 true, ptr {flag}");
            if module
                .functions
                .iter()
                .any(|function| function.name == init_name)
            {
                let init = llvm_function_symbol(&init_name);
                let _ = writeln!(text, "  call void @{init}()");
            }
            let _ = writeln!(text, "  br label %initdone{tag}");
            state.control_flow.label(text, format!("initdone{tag}"));
        }
        Instruction::LoadStatic {
            destination,
            class,
            field,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let global = static_global_name(class, field);
            let _ = writeln!(text, "  %v{} = load {llvm_ty}, ptr {global}", destination.0);
        }
        Instruction::StoreStatic {
            class,
            field,
            value,
            ty,
            ..
        } => {
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated static value type");
            let operand = coerce_to_type(text, *value, value_ty, ty);
            let global = static_global_name(class, field);
            let _ = writeln!(text, "  store {llvm_ty} {operand}, ptr {global}");
        }
        Instruction::SetMember {
            object,
            name,
            owner,
            value,
            ty,
            ..
        } => {
            let offset = field_byte_offset(module, owner, name);
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated member value type");
            emit_set_member(text, *object, offset, *value, value_ty, ty, state);
        }
        Instruction::SetField {
            symbol,
            path,
            value,
            ty,
            ..
        } => {
            let owner = match analysis.symbols.get(symbol) {
                Some(Type::Named(name) | Type::ImportedNamed { name, .. }) => name.as_str(),
                _ => "Box",
            };
            let field = path.first().map_or("value", String::as_str);
            let offset = field_byte_offset(module, owner, field);
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated field value type");
            let _ = writeln!(
                text,
                "  %fieldobj{} = load ptr, ptr %s{}",
                value.0, symbols[symbol]
            );
            // Reuse SetMember emitter with a synthetic object value id name via temp.
            let llvm_ty = llvm_type(ty).expect("validated field type");
            let value_op = coerce_to_type(text, *value, value_ty, ty);
            let _ = writeln!(
                text,
                "  %fieldptr{} = getelementptr i8, ptr %fieldobj{}, i32 {offset}",
                value.0, value.0
            );
            let _ = writeln!(
                text,
                "  store {llvm_ty} {value_op}, ptr %fieldptr{}",
                value.0
            );
        }
        Instruction::SetFieldIndex {
            symbol,
            path,
            indices,
            value,
            ty,
            ..
        } => {
            let owner = match analysis.symbols.get(symbol) {
                Some(Type::Named(name) | Type::ImportedNamed { name, .. }) => name.as_str(),
                _ if function.parameters.first() == Some(symbol) => function
                    .name
                    .rsplit_once('.')
                    .map(|(class, _)| class)
                    .expect("validated method owner"),
                _ => unreachable!("validated indexed field owner"),
            };
            let field = path.first().expect("validated indexed field path");
            let offset = field_byte_offset(module, owner, field);
            let index = indices[0];
            emit_field_set_index(
                text,
                block_id,
                symbols[symbol],
                offset,
                index,
                analysis.values.get(&index).expect("validated index type"),
                *value,
                analysis.values.get(value).expect("validated value type"),
                ty,
                state,
            );
        }
        Instruction::Member {
            destination,
            object,
            name,
            owner,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            if owner == "Error" && name == "Message" {
                let aggregate = analysis
                    .values
                    .get(object)
                    .and_then(llvm_type)
                    .expect("validated error aggregate");
                let _ = writeln!(
                    text,
                    "  %v{} = extractvalue {aggregate} %v{}, 1",
                    destination.0, object.0
                );
            } else if owner == "Error" && name == "Code" {
                let _ = writeln!(
                    text,
                    "  %errorcode{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    destination.0, object.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = trunc i64 %errorcode{} to i32",
                    destination.0, destination.0
                );
            } else {
                let offset = field_byte_offset(module, owner, name);
                emit_member(text, *destination, *object, offset, ty);
            }
        }
        Instruction::SetIndex {
            symbol,
            indices,
            value,
            ty,
            ..
        } => {
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated setindex value type");
            if indices.len() == 1 {
                let index = indices[0];
                let index_ty = analysis
                    .values
                    .get(&index)
                    .expect("validated setindex index type");
                emit_pointer_set_index(
                    text,
                    block_id,
                    symbols[symbol],
                    index,
                    index_ty,
                    *value,
                    value_ty,
                    ty,
                    state,
                );
            } else {
                emit_vector_set_indices(
                    text,
                    block_id,
                    symbols[symbol],
                    indices,
                    *value,
                    value_ty,
                    ty,
                    analysis,
                    state,
                );
            }
        }
        Instruction::Index {
            destination,
            object,
            index,
            ty,
            ..
        } if analysis
            .values
            .get(object)
            .is_some_and(|ty| is_native_vector(ty) || is_native_pointer(ty)) =>
        {
            block_state.constants.remove(destination);
            let index_ty = analysis
                .values
                .get(index)
                .expect("validated vector index type");
            emit_vector_index(
                text,
                block_id,
                *destination,
                *object,
                *index,
                index_ty,
                ty,
                state,
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::String) => {
            block_state.constants.remove(destination);
            let idx = extend_to_i32_index(
                text,
                *index,
                analysis.values.get(index).expect("validated index type"),
            );
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %strindexpacked{dest} = call i64 @bn_rt_str_index_utf8(ptr %v{}, i32 {idx})",
                object.0
            );
            let _ = writeln!(text, "  %strindexbuffer{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  store i64 %strindexpacked{dest}, ptr %strindexbuffer{dest}"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = getelementptr i8, ptr %strindexbuffer{dest}, i64 0"
            );
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if analysis.values.get(object) == Some(&Type::HostArgs) => {
            block_state.constants.remove(destination);
            let index_type = analysis.values.get(index).expect("validated index type");
            let index =
                coerce_to_type(text, *index, index_type, &Type::Integer(IntegerType::Int32));
            let _ = writeln!(
                text,
                "  %argptr{} = getelementptr ptr, ptr %argv, i32 {index}",
                destination.0
            );
            let _ = writeln!(
                text,
                "  %v{} = load ptr, ptr %argptr{}",
                destination.0, destination.0
            );
        }
        Instruction::Print {
            values: printed, ..
        } => {
            let stdout = format!("%stdout{}", state.print_count);
            if state.synchronize_prints {
                let stdout_sym = crate::helpers::stdout_file_symbol();
                let _ = writeln!(text, "  {stdout} = load ptr, ptr @{stdout_sym}");
                let _ = writeln!(text, "  call void @flockfile(ptr {stdout})");
            }
            for (index, value) in printed.iter().enumerate() {
                if index > 0 {
                    let _ = writeln!(
                        text,
                        "  %separator{} = call i32 @putchar(i32 32)",
                        state.print_count
                    );
                    state.print_count += 1;
                }
                lower_print_value(
                    text,
                    *value,
                    analysis
                        .values
                        .get(value)
                        .expect("validated printable type"),
                    state,
                );
                if analysis.owned_string_results.contains(value) {
                    let id = value.0;
                    let _ = writeln!(
                        text,
                        "  %ownedstringerror{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringptr{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringfree{id} = select i1 %ownedstringerror{id}, ptr null, ptr %ownedstringptr{id}"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringna{id} = icmp eq ptr %ownedstringfree{id}, @.bn_na"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringstorage{id} = select i1 %ownedstringna{id}, ptr null, ptr %ownedstringfree{id}"
                    );
                    let _ = writeln!(text, "  call void @free(ptr %ownedstringstorage{id})");
                }
            }
            let _ = writeln!(
                text,
                "  %newline{} = call i32 @putchar(i32 10)",
                state.print_count
            );
            state.print_count += 1;
            if state.synchronize_prints {
                let _ = writeln!(text, "  call void @funlockfile(ptr {stdout})");
            }
        }
        _ => unreachable!(
            "all instructions must be validated before emission: {}",
            instruction_name(instruction)
        ),
    }
    Ok(())
}

fn extend_to_i32_index(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated index type") {
        "i32" => format!("%v{}", value.0),
        "i64" => {
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        llvm_ty => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("stridx{}", value.0);
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i32", value.0);
            format!("%{temp}")
        }
    }
}
