// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::{
    fs,
    sync::{Arc, atomic::Ordering},
};
#[test]
fn local_dataframe_class_is_not_a_bndata_intrinsic() {
    let source = "CLASS DataFrame\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nPUBLIC FUNCTION RowCount() AS INTEGER\nRETURN 42\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET frame AS DataFrame = NEW DataFrame()\nPRINT frame.RowCount()\nRELEASE frame\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute local DataFrame");
    assert_eq!(output, "42\n");
}

#[test]
fn bndata_import_constructs_and_releases_empty_frame() {
    let path = "tests/grammar/valid/bndata-import.bn";
    let (_, output) = run_path(path).expect("execute BNData import");
    assert_eq!(output, "0 0\n");
}

#[test]
fn bndata_frame_adds_columns_and_reports_counts() {
    let (_, output) =
        run_path("tests/grammar/valid/bndata-columns.bn").expect("add DataFrame columns");
    assert_eq!(output, "2 2 Age\n");
}

#[test]
fn dataframe_is_test_and_identity_equality() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", ids)\nLET selected AS Data.DataFrame OR Error = table.Select([0], [0])\nIF selected IS Data.DataFrame THEN\nPRINT \"frame\"\nLET alias AS Data.DataFrame OR Error = selected\nIF alias = selected THEN\nPRINT \"same\"\nEND IF\nRELEASE selected\nEND IF\nLET other AS Data.DataFrame = NEW Data.DataFrame()\nIF table = other THEN\nPRINT \"diff\"\nEND IF\nRELEASE table\nRELEASE other\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("IS DataFrame");
    assert_eq!(output, "frame\nsame\n");
}

#[test]
fn bndata_set_label_renames_a_column_in_place() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [7]\ntable.AddIntegerColumn(\"old\", values)\ntable.SetLabel(\"old\", \"new\")\nPRINT table.ColumnName(0)\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("rename DataFrame label");
    assert_eq!(output, "new\n");
}

#[test]
fn bndata_transpose_returns_string_columns() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[2] = [1, 2]\nLET names AS STRING[2] = [\"Ana\", \"Bia\"]\ntable.AddIntegerColumn(\"Id\", ids)\ntable.AddStringColumn(\"Name\", names)\nLET result AS Data.DataFrame OR Error = table.Transpose()\nIF result IS Error THEN\nPRINT result.Code\nELSE\nPRINT result.GetString(0, \"Column\"), result.GetString(1, \"Row0\"), result.GetString(1, \"Row1\")\nRELEASE result\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("transpose DataFrame");
    assert_eq!(output, "Id Ana Bia\n");
}

#[test]
fn bndata_append_rows_returns_a_new_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET first AS INTEGER[1] = [1]\nLET second AS INTEGER[1] = [2]\nleft.AddIntegerColumn(\"Id\", first)\nright.AddIntegerColumn(\"Id\", second)\nLET joined AS Data.DataFrame OR Error = left.AppendRows(right)\nIF joined IS Error THEN\nPRINT joined.Code\nELSE\nPRINT left.RowCount(), joined.RowCount(), joined.GetInteger(1, \"Id\")\nRELEASE joined\nEND IF\nRELEASE left\nRELEASE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("append DataFrame rows");
    assert_eq!(output, "1 2 2\n");
}

#[test]
fn bndata_append_columns_returns_a_new_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[1] = [1]\nLET names AS STRING[1] = [\"Ana\"]\nleft.AddIntegerColumn(\"Id\", ids)\nright.AddStringColumn(\"Name\", names)\nLET joined AS Data.DataFrame OR Error = left.AppendColumns(right)\nIF joined IS Error THEN\nPRINT joined.Code\nELSE\nPRINT left.ColumnCount(), joined.ColumnCount(), joined.GetString(0, \"Name\")\nRELEASE joined\nEND IF\nRELEASE left\nRELEASE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("append DataFrame columns");
    assert_eq!(output, "1 2 Ana\n");
}

#[test]
fn bndata_join_variants_preserve_their_outer_side() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET leftIds AS INTEGER[2] = [1, 2]\nLET scores AS INTEGER[2] = [10, 20]\nLET rightIds AS INTEGER[2] = [2, 3]\nLET names AS STRING[2] = [\"Bia\", \"Caio\"]\nleft.AddIntegerColumn(\"Id\", leftIds)\nleft.AddIntegerColumn(\"Score\", scores)\nright.AddIntegerColumn(\"Id\", rightIds)\nright.AddStringColumn(\"Name\", names)\nLET inner AS Data.DataFrame OR Error = left.Join(right, \"Id\", \"Id\")\nLET leftJoin AS Data.DataFrame OR Error = left.LeftJoin(right, \"Id\", \"Id\")\nLET rightJoin AS Data.DataFrame OR Error = left.RightJoin(right, \"Id\", \"Id\")\nLET full AS Data.DataFrame OR Error = left.FullJoin(right, \"Id\", \"Id\")\nIF inner IS Error OR leftJoin IS Error OR rightJoin IS Error OR full IS Error THEN\nPRINT \"error\"\nELSE\nPRINT inner.RowCount(), leftJoin.RowCount(), rightJoin.RowCount(), full.RowCount(), full.GetInteger(2, \"Id\")\nLET missing AS INTEGER OR NA OR Error = full.GetInteger(2, \"Score\")\nPRINT missing\nRELEASE inner\nRELEASE leftJoin\nRELEASE rightJoin\nRELEASE full\nEND IF\nRELEASE left\nRELEASE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("join DataFrames");
    assert_eq!(output, "1 2 2 3 3\nNA\n");
}

#[test]
fn bndata_read_csv_builds_string_columns() {
    let (_, output) = run_path("tests/grammar/valid/bndata-csv.bn").expect("read CSV");
    assert_eq!(
        output,
        "2 2 age\nAna\n35 31.5\n31.5\n31.5 28.0 35.0\n2 2\n1 1\n"
    );
}

#[test]
fn bndata_read_csv_uses_the_injected_data_provider() {
    let path = std::env::temp_dir().join(format!(
        "basicnext-provider-{}-{}.csv",
        std::process::id(),
        NEXT_PROVIDER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, "ignored\n").expect("write provider fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nRETURN\nEND IF\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, FALSE, \",\")\nIF table IS Error THEN\nRETURN\nEND IF\nPRINT table.GetString(0, \"Column1\")\nRELEASE table\nfile.Close()\nRELEASE file\nEND FUNCTION\n",
        bn_path(&path)
    );
    let host =
        fixed(vec!["provider.bn".into()], 0, 0).with_data_provider(Arc::new(InjectedDataProvider));
    let result = run_with_host(&source, "", &host);
    let _ = fs::remove_file(path);
    let (_, output) = result.expect("injected data provider");
    assert_eq!(output, "provided\n");
}

#[test]
fn bndata_read_csv_rejects_ragged_rows() {
    let csv = unique_temp("ragged.csv");
    fs::write(&csv, "name;score\nAna;10\nCarlos\n").expect("create ragged CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \";\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nRELEASE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("ragged CSV returns Error");
    assert_eq!(output, "6\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_rejects_unterminated_quotes() {
    let csv = unique_temp("unquoted.csv");
    fs::write(&csv, "name\n\"Ana\n").expect("create unterminated CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nRELEASE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("unterminated CSV quotes return Error");
    assert_eq!(output, "6\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_write_csv_on_a_closed_file_returns_error() {
    let csv = unique_temp("writecsv-closed.csv");
    let path = bn_path(&csv);
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET names AS STRING[1] = [\"Ana\"]\ntable.AddStringColumn(\"name\", names)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nfile.Close()\nLET written AS VOID OR Error = Data.WriteCSV(file, table, TRUE, \",\")\nIF written IS Error THEN\nPRINT written.Code\nEND IF\nRELEASE file\nEND IF\nRELEASE table\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("WriteCSV on a closed file returns Error");
    assert_eq!(output, "5\n"); // FS.CLOSED from the closed file; BNData codes: bucket 0.6.2b R8
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_rejects_invalid_separator() {
    let source = "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \"\\\"\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nRELEASE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("invalid separator returns Error");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_conversion_failure_leaves_the_column_unchanged() {
    let csv = unique_temp("conversion.csv");
    fs::write(&csv, "value\n10\n999999999999999999999\n").expect("create conversion CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \";\")\nIF table IS Error THEN\nPRINT table.Code\nELSE\nLET converted AS VOID OR Error = table.ConvertToInteger(\"value\")\nIF converted IS Error THEN\nLET first AS STRING OR NA OR Error = table.GetString(0, \"value\")\nPRINT first\nEND IF\nRELEASE table\nEND IF\nRELEASE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("conversion failure is an Error");
    assert_eq!(output, "10\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_copy_failure_leaves_the_destination_unchanged() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"1\", \"\"]\nLET target AS POINTER TO INTEGER[] = NEW INTEGER[2]\ntarget[0] = 9\ntarget[1] = 9\ntable.AddStringColumn(\"Id\", text)\ntable.ConvertToInteger(\"Id\")\nLET copied AS VOID OR Error = table.CopyIntegerColumn(\"Id\", target)\nIF copied IS Error THEN\nPRINT target[0], target[1]\nEND IF\nRELEASE target\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run atomic copy");
    assert_eq!(output, "9 9\n");
}

#[test]
fn bndata_zscore_standardizes_a_float_column() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[3] = [1.0, 2.0, 3.0]\ntable.AddFloatColumn(\"x\", xs)\nLET z AS Data.DataFrame OR Error = table.ZScore(\"x\")\nIF z IS Error THEN\nPRINT z.Code\nELSE\nPRINT z.GetFloat(0, \"x\")\nPRINT z.GetFloat(1, \"x\")\nPRINT z.GetFloat(2, \"x\")\nRELEASE z\nEND IF\nLET missing AS Data.DataFrame OR Error = table.ZScore(\"nope\")\nIF missing IS Error THEN\nPRINT missing.Code\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run ZScore");
    assert_eq!(output, "-1.0\n0.0\n1.0\n2\n");
}

#[test]
fn bndata_select_rejects_out_of_bounds_indices() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", values)\nLET result AS Data.DataFrame OR Error = table.Select([1], [0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run bounds check");
    assert_eq!(output, "4\n");
}

#[test]
fn bndata_select_rejects_negative_indices_as_error() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", values)\nLET rows AS INTEGER[1]\nrows[0] = -1\nLET result AS Data.DataFrame OR Error = table.Select(rows, [0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("negative Select is Error");
    assert_eq!(output, "4\n");
}

#[test]
fn bndata_slice_rejects_row_range_on_empty_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET sliced AS Data.DataFrame OR Error = table.Slice(0, 1, 0, 0)\nIF sliced IS Error THEN\nPRINT sliced.Code\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty-frame Slice is Error");
    assert_eq!(output, "4\n");
}

#[test]
fn bndata_empty_and_all_na_reductions_follow_bnmath() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET empty AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[0] = []\nempty.AddFloatColumn(\"x\", xs)\nPRINT empty.Mean(\"x\")\nPRINT empty.Median(\"x\")\nPRINT empty.Range(\"x\")\nRELEASE empty\nLET nas AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"\", \"\"]\nnas.AddStringColumn(\"n\", text)\nnas.ConvertToInteger(\"n\")\nPRINT nas.Mean(\"n\")\nPRINT nas.Quartile1(\"n\")\nLET mode AS FLOAT OR NA OR Error = nas.Mode(\"n\")\nPRINT mode\nRELEASE nas\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty and all-NA reductions");
    assert_eq!(output, "NAN\nNAN\nNAN\nNAN\nNAN\nNA\n");
}

#[test]
fn bndata_deleted_frame_rejects_reuse() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nRELEASE table\nPRINT table.RowCount()\nEND FUNCTION\n";
    let error = run(source, "").expect_err("reject deleted frame");
    assert_eq!(error.code, "USE_AFTER_RELEASE");
}

#[test]
fn bndata_write_csv_serializes_headers_and_rows() {
    let csv = unique_temp("sprint6-write.csv");
    let path = bn_path(&csv);
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET frame AS Data.DataFrame = NEW Data.DataFrame()\nLET names AS STRING[2] = [\"Ana\", \"Carlos\"]\nframe.AddStringColumn(\"name\", names)\nLET file AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET result AS VOID OR Error = Data.WriteCSV(file, frame, TRUE, \",\")\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nfile.Close()\nRELEASE file\nEND IF\nRELEASE frame\nEND FUNCTION\n"
    );
    run(&source, "").expect("write CSV");
    let text = fs::read_to_string(&csv).expect("read written CSV");
    assert_eq!(text, "name\nAna\nCarlos\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_accepts_variable_fixed_vector_lengths() {
    let (_, output) =
        run_path("tests/grammar/valid/bndata-variable-length.bn").expect("variable vector length");
    assert_eq!(output, "3\n");
}

#[test]
fn error_return_narrows_dataframe_for_readcsv() {
    let source = "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"tests/fixtures/bndata-sprint6.csv\", FS.READ)\nIF file IS Error THEN\nRETURN\nEND IF\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nfile.Close()\nRELEASE file\nRETURN\nEND IF\nPRINT table.RowCount()\nRELEASE table\nfile.Close()\nRELEASE file\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("narrow DataFrame after RETURN");
    assert_eq!(output, "2\n");
}

#[test]
fn bndata_zscore_empty_and_all_na_follow_bnmath() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET empty AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[0] = []\nempty.AddFloatColumn(\"x\", xs)\nLET z AS Data.DataFrame OR Error = empty.ZScore(\"x\")\nIF z IS Error THEN\nPRINT \"empty-error\"\nELSE\nPRINT z.RowCount()\nRELEASE z\nEND IF\nRELEASE empty\nLET nas AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"\", \"\"]\nnas.AddStringColumn(\"n\", text)\nnas.ConvertToInteger(\"n\")\nLET z2 AS Data.DataFrame OR Error = nas.ZScore(\"n\")\nIF z2 IS Error THEN\nPRINT \"na-error\"\nELSE\nPRINT z2.GetFloat(0, \"n\")\nRELEASE z2\nEND IF\nRELEASE nas\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("ZScore empty and all-NA");
    assert_eq!(output, "0\nNA\n");
}

#[test]
fn bndata_getstring_returns_na_for_unmatched_join_cells() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET leftIds AS INTEGER[1] = [1]\nLET names AS STRING[1] = [\"Ana\"]\nLET rightIds AS INTEGER[1] = [2]\nLET extra AS STRING[1] = [\"Bia\"]\nleft.AddIntegerColumn(\"Id\", leftIds)\nleft.AddStringColumn(\"Name\", names)\nright.AddIntegerColumn(\"Id\", rightIds)\nright.AddStringColumn(\"Other\", extra)\nLET full AS Data.DataFrame OR Error = left.FullJoin(right, \"Id\", \"Id\")\nIF full IS Error THEN\nPRINT \"join-error\"\nELSE\nLET missing AS STRING OR NA OR Error = full.GetString(1, \"Name\")\nPRINT missing\nRELEASE full\nEND IF\nRELEASE left\nRELEASE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("GetString NA from join");
    assert_eq!(output, "NA\n");
}

#[test]
fn bndata_select_rejects_duplicate_column_indices() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[2] = [1, 2]\ntable.AddIntegerColumn(\"Id\", values)\nLET result AS Data.DataFrame OR Error = table.Select([0, 1], [0, 0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nRELEASE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("duplicate Select columns");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_read_csv_rejects_duplicate_headers() {
    let csv = unique_temp("dup-header.csv");
    fs::write(&csv, "name,name\nAna,Bia\n").expect("create duplicate-header CSV");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nRELEASE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("duplicate CSV headers");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_ignores_a_trailing_blank_line() {
    let csv = unique_temp("trailing-blank.csv");
    fs::write(&csv, "name,age\nAna,28\n\n").expect("create trailing-blank CSV");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT \"error\"\nELSE\nPRINT table.RowCount()\nRELEASE table\nEND IF\nfile.Close()\nRELEASE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("trailing blank CSV");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}
