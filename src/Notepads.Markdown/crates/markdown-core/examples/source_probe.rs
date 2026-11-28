// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Finite desktop Release source preparation probe; not a parser or UWP product test.

use markdown_core::identity::SourceKey;
use markdown_core::policy::{CancellationToken, SourceBudget};
use markdown_core::source::{ParseInput, SourceDocument};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 || !matches!(args[1].as_str(), "warm" | "cold") {
        return Err("usage: source_probe warm|cold samples input-path".into());
    }
    let samples: usize = args[2].parse()?;
    if !(1..=100).contains(&samples) || (args[1] == "cold" && samples != 1) {
        return Err("finite samples must be 1..100; cold is one fresh-process operation".into());
    }
    let bytes = std::fs::read(&args[3])?;
    let budget = SourceBudget::source_fixture();
    let warmup = if args[1] == "warm" { 5 } else { 0 };
    println!(
        "sample\tmilliseconds\tsource_bytes\tparse_bytes\tpieces\tidentity\tscanned\tcopied\tevents\tsource_capacity_peak\tauxiliary_capacity_peak"
    );
    for index in 0..samples + warmup {
        let mut op = budget.begin(CancellationToken::new());
        let started = Instant::now();
        let document = SourceDocument::new(SourceKey::new([9; 16], 1)?, &bytes, &mut op)?;
        let prepared = ParseInput::prepare(document.snapshot(), &mut op)?;
        let parse_bytes = prepared.text().len();
        let pieces = prepared.pieces().len();
        let identity = prepared.is_identity();
        drop(prepared);
        drop(document);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let usage = budget.usage();
        if usage.source_capacity != 0 || usage.auxiliary_capacity != 0 || usage.source_owners != 0 {
            return Err("operation retained a charged owner after cleanup".into());
        }
        if index >= warmup {
            let stats = op.stats().expect("statistics enabled");
            println!(
                "{}\t{elapsed:.6}\t{}\t{parse_bytes}\t{pieces}\t{identity}\t{}\t{}\t{}\t{}\t{}",
                index - warmup,
                bytes.len(),
                stats.scanned_bytes(),
                stats.copied_bytes(),
                stats.events(),
                usage.source_capacity_peak,
                usage.auxiliary_capacity_peak,
            );
        }
    }
    Ok(())
}
