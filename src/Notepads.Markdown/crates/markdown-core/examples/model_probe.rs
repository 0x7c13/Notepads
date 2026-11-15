// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One finite desktop C0 acceptance measurement; not UWP or G3/G4 acceptance.

use markdown_core::config::{ParseConfig, Profile};
use markdown_core::identity::{LogicalBoundary, SourceBoundary, SourceKey, SourceSpan};
use markdown_core::ir::{AtomKind, TextTarget};
use markdown_core::mapping::{
    Affinity, Correspondence, PointInput, PointQuery, Position, RangeInput, RangeQuery, RangeResult,
};
use markdown_core::model::AcceptedModel;
use markdown_core::policy::{CancellationToken, Operation, SourceBudget};
use markdown_core::source::SourceDocument;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const USAGE: &str = "usage: model_probe input-path [--verify-identity-copy]";
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or(USAGE)?;
    let verify = match args.next() {
        None => false,
        Some(flag) if flag == "--verify-identity-copy" => true,
        _ => return Err(USAGE.into()),
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let bytes = std::fs::read(path)?;
    // A harness precondition outside parse timing/fees, never a product rule.
    if verify && (bytes.is_empty() || !bytes.is_ascii()) {
        return Err("identity-copy verification requires nonempty ASCII input".into());
    }
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let start = Instant::now();
    let result =
        SourceDocument::new(SourceKey::new([9; 16], 1)?, &bytes, &mut op).and_then(|document| {
            AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::C0),
                1,
                &mut op,
            )
        });
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "profile\tstatus\tmilliseconds\tsource_bytes\tnodes\tatoms\tlogical_utf16\tscanned\tcopied\tevents\tsource_capacity_peak\tauxiliary_capacity_peak\terror\tcharged_units\tmetadata_scanned\tmetadata_copied\tmetadata_actions\tbyte_event_total"
    );
    let (status, nodes, atoms, logical, error) = match &result {
        Ok(model) => (
            "accepted",
            model.ir().nodes().len(),
            model.ir().atoms_len(),
            model.ir().flows()[0].logical_utf16,
            String::new(),
        ),
        Err(error) => ("rejected", 0, 0, 0, format!("{error:?}")),
    };
    let stats = op.stats().expect("statistics enabled");
    let usage = budget.usage();
    println!(
        "C0\t{status}\t{elapsed:.6}\t{}\t{nodes}\t{atoms}\t{logical}\t{}\t{}\t{}\t{}\t{}\t{error}\t{}\t{}\t{}\t{}\t{}",
        bytes.len(),
        stats.scanned_bytes(),
        stats.copied_bytes(),
        stats.events(),
        usage.source_capacity_peak,
        usage.auxiliary_capacity_peak,
        stats.charged_units(),
        stats.metadata_scanned_bytes(),
        stats.metadata_copied_bytes(),
        stats.metadata_actions(),
        stats.total()
    );
    let verification = if verify {
        match &result {
            Ok(model) => verify_identity_copy(model, bytes.len() as u64, &mut op),
            Err(_) => Err("identity-copy verification requires an accepted model".into()),
        }
    } else {
        Ok(())
    };
    if verify {
        eprintln!(
            "VERIFY_IDENTITY_COPY\tstatus={}\tpost_query_charged_units={}\terror={}",
            if verification.is_ok() {
                "passed"
            } else {
                "failed"
            },
            op.stats().expect("statistics enabled").charged_units(),
            verification
                .as_ref()
                .err()
                .map_or(String::new(), ToString::to_string)
        );
    }
    drop(result);
    let usage = budget.usage();
    if usage.source_capacity != 0 || usage.auxiliary_capacity != 0 || usage.source_owners != 0 {
        return Err("operation retained a charged owner after cleanup".into());
    }
    eprintln!("cleanup source=0 auxiliary=0 owners=0");
    verification?;
    Ok(())
}

fn verify_identity_copy(
    model: &AcceptedModel,
    length: u64,
    op: &mut Operation,
) -> Result<(), Box<dyn std::error::Error>> {
    let flow = model.main_flow_id();
    let target = TextTarget::Flow(flow);
    if model
        .ir()
        .flows()
        .get(flow.index())
        .ok_or("missing Main flow")?
        .logical_utf16
        != length
    {
        return Err("identity-copy Main length mismatch".into());
    }
    let mut points = vec![0, length];
    points.extend(
        [1023, 1024, 1025, length - 1]
            .into_iter()
            .filter(|p| *p <= length),
    );
    points.sort_unstable();
    points.dedup();
    for offset in &points {
        for (input, expected, direction) in [
            (
                PointInput::Source {
                    target,
                    byte: SourceBoundary::new(*offset),
                },
                Position::Flow(LogicalBoundary::new(*offset)),
                "source_to_main",
            ),
            (
                PointInput::Flow {
                    flow,
                    utf16: LogicalBoundary::new(*offset),
                },
                Position::Source(SourceBoundary::new(*offset)),
                "main_to_source",
            ),
        ] {
            let response = model.map_point(
                &PointQuery {
                    model: model.identity(),
                    input,
                    affinity: Affinity::Before,
                },
                op,
            )?;
            if response.results().len() != 1
                || response.results()[0].chosen != expected
                || response.results()[0].correspondence != Correspondence::Exact
            {
                return Err(
                    format!("identity-copy point mismatch at {offset}, {direction}").into(),
                );
            }
            eprintln!("VERIFY_POINT\t{direction}\t{offset}\tExact");
        }
    }
    let response = model.map_range(
        &RangeQuery {
            model: model.identity(),
            input: RangeInput::Source {
                target,
                span: SourceSpan::new(0, length)?,
            },
            empty_affinity: Affinity::Before,
        },
        op,
    )?;
    let [RangeResult::Selection(rows)] = response.results() else {
        return Err("identity-copy range did not yield a selection".into());
    };
    let end = rows
        .start()
        .checked_add(rows.len())
        .ok_or("selection bounds overflow")?;
    let [selection] = response
        .selections()
        .get(rows.start()..end)
        .ok_or("invalid selection bounds")?
    else {
        return Err("identity-copy range did not yield exactly one selection".into());
    };
    let rows = selection.contributions;
    let end = rows
        .start()
        .checked_add(rows.len())
        .ok_or("contribution bounds overflow")?;
    let [contribution] = response
        .contributions()
        .get(rows.start()..end)
        .ok_or("invalid contribution bounds")?
    else {
        return Err("identity-copy range did not yield exactly one contribution".into());
    };
    if selection.expanded
        || selection.start.chosen != Position::Flow(LogicalBoundary::new(0))
        || selection.end.chosen != Position::Flow(LogicalBoundary::new(length))
        || selection.start.correspondence != Correspondence::Exact
        || selection.end.correspondence != Correspondence::Exact
        || contribution.expanded
        || contribution.kind != AtomKind::Copy
        || contribution.source != SourceSpan::new(0, length)?
        || contribution.target.start != 0
        || contribution.target.end != length
    {
        return Err("identity-copy full-range geometry mismatch".into());
    }
    eprintln!("VERIFY_RANGE\tsource_to_main\t0\t{length}\tCopy\texpanded=false");
    Ok(())
}
