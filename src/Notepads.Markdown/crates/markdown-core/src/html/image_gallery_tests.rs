// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::tests::parse;
use crate::identity::{PayloadBoundary, SourceBoundary};
use crate::ir::ObjectRow;
use crate::mapping::{Affinity, Correspondence, PointInput, PointQuery, Position};
use crate::policy::CancellationToken;

#[test]
fn html_image_galleries_cross_destination_sort_work_windows() {
    for count in [32, 42, 43, 64, 128] {
        for linked in [false, true] {
            let mut source = String::from("<p>");
            for index in 0..count {
                if linked {
                    source.push_str("<a href='https://example.org'>");
                }
                source.push_str(&format!("<img src='icon-{index}.png' alt='item {index}'>"));
                if linked {
                    source.push_str("</a>");
                }
            }
            source.push_str("</p>");
            let model = parse(&source, true);
            let mut images = model
                .ir()
                .objects()
                .iter()
                .filter_map(|object| {
                    if let ObjectRow::Image {
                        node, destination, ..
                    } = *object
                    {
                        Some((
                            model.ir().nodes()[node.index()].source_envelope.start(),
                            destination,
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            images.sort_unstable_by_key(|&(start, _)| start);
            assert_eq!(images.len(), count);
            for (index, &(_, payload)) in images.iter().enumerate() {
                let expected = source.find(&format!("icon-{index}.png")).unwrap() as u64 + 1;
                let mut op = model.budget().begin(CancellationToken::new());
                let response = model
                    .map_point(
                        &PointQuery {
                            model: model.identity(),
                            input: PointInput::Payload {
                                payload,
                                utf8: PayloadBoundary::new(1),
                            },
                            affinity: Affinity::After,
                        },
                        &mut op,
                    )
                    .unwrap();
                assert_eq!(
                    response.results()[0].chosen,
                    Position::Source(SourceBoundary::new(expected))
                );
                assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
            }
        }
    }
}
