#![no_main]

use libfuzzer_sys::fuzz_target;
use topos_lib::invariants;

fuzz_target!(|input: &str| {
    invariants::check(input);
});
