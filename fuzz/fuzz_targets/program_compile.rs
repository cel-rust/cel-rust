#![no_main]

use cel::Program;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| {
    let _ = Program::compile(input);
});
