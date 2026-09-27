#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let source = bn_source::SourceFile::new("fuzz.bn", text);
    let _ = bn_frontend::lexer::lex(&source);
});
