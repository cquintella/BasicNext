// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Differential test of ARC (bucket typed-llvm-emitter, Sprint 8, phase A):
//! reproducible pseudo-random programs (the seed is in the failure message)
//! of creations, aliases, reassignments, `NULL`, and field stores, run on
//! `bni` and on a `bnc` executable, and compared with a model of the rules
//! of `0.6.md` ("Memory model (ARC)") written here: an assignment retains
//! the new value and then releases the previous one; the end of `Start`
//! releases its locals in reverse declaration order; a destroyed holder
//! releases its field. Every destructor prints, so the output is the exact
//! order of destruction.

mod support;

use std::{fmt::Write as _, time::Duration};

use support::{TestDir, bnc, bni, run};

const SLOTS: usize = 4;
const STEPS: usize = 30;
const SEEDS: u64 = 40;

/// xorshift64*: a fixed generator, so a seed always yields the same program.
struct Random(u64);

impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let value = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D);
        usize::try_from(value % u64::try_from(bound).expect("bound fits u64")).expect("fits usize")
    }
}

/// The rules, on object numbers: strong counts, the locals, the field.
#[derive(Default)]
struct Model {
    counts: std::collections::BTreeMap<usize, usize>,
    slots: [Option<usize>; SLOTS],
    field: Option<usize>,
    output: String,
}

impl Model {
    fn retain(&mut self, object: Option<usize>) {
        if let Some(object) = object {
            *self.counts.get_mut(&object).expect("live object") += 1;
        }
    }

    fn release(&mut self, object: Option<usize>) {
        let Some(object) = object else {
            return;
        };
        let count = self.counts.get_mut(&object).expect("live object");
        *count -= 1;
        if *count == 0 {
            self.counts.remove(&object);
            let _ = writeln!(self.output, "free {object}");
        }
    }

    /// `target = value`: the new value is retained, then the old released.
    fn assign_slot(&mut self, slot: usize, value: Option<usize>, owned: bool) {
        if !owned {
            self.retain(value);
        }
        let previous = std::mem::replace(&mut self.slots[slot], value);
        self.release(previous);
    }

    fn assign_field(&mut self, value: Option<usize>) {
        self.retain(value);
        let previous = std::mem::replace(&mut self.field, value);
        self.release(previous);
    }
}

/// One program and the output the model gives it.
fn program(seed: u64) -> (String, String) {
    let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut model = Model::default();
    let mut body = String::from("    LET h AS Holder = NEW Holder()\n");
    for slot in (0..SLOTS).rev() {
        let _ = writeln!(body, "    LET s{slot} AS Box OR NULL = NULL");
    }
    let mut next_object = 0;
    for step in 0..STEPS {
        let slot = random.next(SLOTS);
        match random.next(5) {
            0 | 1 => {
                let _ = writeln!(body, "    s{slot} = NEW Box({next_object})");
                model.counts.insert(next_object, 1);
                model.assign_slot(slot, Some(next_object), true);
                next_object += 1;
            }
            2 => {
                let other = random.next(SLOTS);
                let _ = writeln!(body, "    s{slot} = s{other}");
                model.assign_slot(slot, model.slots[other], false);
            }
            3 => {
                let _ = writeln!(body, "    s{slot} = NULL");
                model.assign_slot(slot, None, false);
            }
            _ => {
                if random.next(2) == 0 {
                    let _ = writeln!(body, "    h.slot = s{slot}");
                    model.assign_field(model.slots[slot]);
                } else {
                    let _ = writeln!(body, "    h.slot = NULL");
                    model.assign_field(None);
                }
            }
        }
        let _ = writeln!(body, "    PRINT \"step {step}\"");
        let _ = writeln!(model.output, "step {step}");
    }
    // The end of Start: locals in reverse declaration order; the holder
    // last, which releases its field.
    for slot in 0..SLOTS {
        let previous = model.slots[slot].take();
        model.release(previous);
    }
    let field = model.field.take();
    model.release(field);
    let source = format!(
        "CLASS Box\n    PUBLIC n AS INTEGER = 0\n    PUBLIC FUNCTION CONSTRUCTOR(n AS INTEGER)\n        SELF.n = n\n    END FUNCTION\n    PUBLIC FUNCTION DESTRUCTOR()\n        PRINT \"free\", SELF.n\n    END FUNCTION\nEND CLASS\n\nCLASS Holder\n    PUBLIC slot AS Box OR NULL = NULL\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\nFUNCTION Start() AS VOID\n{body}END FUNCTION\n"
    );
    (source, model.output)
}

#[test]
fn random_ownership_programs_match_the_model_on_both_backends() {
    let directory = TestDir::new("arc-differential").expect("create test directory");
    let timeout = Duration::from_secs(60);
    let mut failures = Vec::new();
    for seed in 0..SEEDS {
        let (source, expected) = program(seed);
        let path = directory.join(format!("seed{seed}.bn"));
        std::fs::write(&path, &source).expect("write program");
        let mut interpreted = run(bni().arg("run").arg(&path), None, timeout).expect("run bni");
        interpreted.accept_expected_status();
        let artifact = directory.join(format!("seed{seed}{}", std::env::consts::EXE_SUFFIX));
        let mut built =
            run(bnc().arg(&path).arg("-o").arg(&artifact), None, timeout).expect("run bnc");
        built.accept_expected_status();
        let compiled = if built.status.success() {
            let mut compiled = run(&mut std::process::Command::new(&artifact), None, timeout)
                .expect("run the compiled program");
            compiled.accept_expected_status();
            String::from_utf8_lossy(&compiled.stdout).into_owned()
        } else {
            format!("build failed: {}", String::from_utf8_lossy(&built.stderr))
        };
        let interpreted = String::from_utf8_lossy(&interpreted.stdout).into_owned();
        if interpreted != expected || compiled != expected {
            failures.push(format!(
                "seed {seed}:\n{source}\n-- model\n{expected}-- bni\n{interpreted}-- bnc\n{compiled}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
