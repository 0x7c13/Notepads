use crate::Result;
use crate::model::{Bounds, PacketBlockLayout, PacketDiagramLayout, PacketWordLayout};
use crate::text::TextMeasurer;
use merman_core::diagrams::packet::PacketDiagramRenderModel;

mod config;

pub(crate) use config::{PacketBitOrder, PacketConfigView};

pub(crate) fn layout_packet_diagram_typed(
    model: &PacketDiagramRenderModel,
    diagram_title: Option<&str>,
    effective_config: &serde_json::Value,
    _measurer: &dyn TextMeasurer,
) -> Result<PacketDiagramLayout> {
    let _ = (model.acc_title.as_deref(), model.acc_descr.as_deref());
    let cfg = PacketConfigView::new(effective_config)
        .layout_settings()
        .map_err(|error| crate::Error::InvalidModel {
            message: error.to_string(),
        })?;

    let total_row_height = cfg.row_height + cfg.padding_y;
    let title_from_semantic = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let title_from_meta = diagram_title.map(str::trim).filter(|t| !t.is_empty());
    let has_title = title_from_semantic.or(title_from_meta).is_some();

    let words_count = model.packet.len();
    let svg_height = total_row_height * ((words_count + 1) as f64)
        - if has_title { 0.0 } else { cfg.row_height };
    let svg_width = cfg.bit_width * (cfg.bits_per_row as f64) + 2.0;

    let mut words: Vec<PacketWordLayout> = Vec::new();
    for (row_number, word) in model.packet.iter().enumerate() {
        let word_y = (row_number as f64) * total_row_height + cfg.padding_y;
        let mut blocks: Vec<PacketBlockLayout> = Vec::new();
        for block in word {
            let bits = block.end - block.start + 1;
            let first_column = block.start % cfg.bits_per_row;
            let column = match cfg.bit_order {
                PacketBitOrder::Ascending => first_column,
                PacketBitOrder::Descending => cfg.bits_per_row - first_column - bits,
            };
            let block_x = column as f64 * cfg.bit_width + 1.0;
            let width = bits as f64 * cfg.bit_width - cfg.padding_x;
            blocks.push(PacketBlockLayout {
                start: block.start,
                end: block.end,
                label: block.label.clone(),
                x: block_x,
                y: word_y,
                width,
                height: cfg.row_height,
            });
        }
        words.push(PacketWordLayout { blocks });
    }

    Ok(PacketDiagramLayout {
        bounds: Some(Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: svg_width,
            max_y: svg_height.max(1.0),
        }),
        width: svg_width,
        height: svg_height.max(1.0),
        row_height: cfg.row_height,
        padding_x: cfg.padding_x,
        padding_y: cfg.padding_y,
        bit_width: cfg.bit_width,
        bits_per_row: cfg.bits_per_row,
        show_bits: cfg.show_bits,
        words,
    })
}
