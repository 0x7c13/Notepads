// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Borrowed native route for the funded flowchart entry. Every operation uses
//! the same DirectWrite backend as HostTextMeasurer. The neutral values below
//! are cancellation sentinels: the native caller must check the shared control
//! immediately after a measurement, and may never publish them as a fallback.

use crate::WindowsText;
use markdown_mermaid::canonical::CanonicalError;
use merman_render::{
    environment::{
        HostTextMeasurement as Value, HostTextMeasurementRequest as Request,
        TextMeasurementOperation as Op, TextMeasurementPhase as Phase,
    },
    text::{TextMeasurer, TextMetrics, TextStyle, WrapMode},
};

impl WindowsText {
    fn native_value(
        &self,
        op: Op,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
        wrap_mode: WrapMode,
    ) -> Option<Value> {
        let phase = match op {
            Op::Measure => Phase::Layout,
            Op::Wrapped | Op::WrappedWithRawWidth => Phase::Wrap,
            Op::ComputedLength | Op::CanvasMeasureTextWidth => Phase::ComputedLength,
            _ => Phase::SvgBBox,
        };
        match self.measurement(Request {
            operation: op,
            phase,
            text,
            style,
            max_width,
            wrap_mode,
        }) {
            Ok(value) => Some(value),
            Err(error) => {
                self.record(error);
                None
            }
        }
    }

    fn native_length(&self, op: Op, text: &str, style: &TextStyle, mode: WrapMode) -> f64 {
        match self.native_value(op, text, style, None, mode) {
            Some(Value::Length(value)) => value,
            None => 0.0,
            Some(_) => {
                self.record(CanonicalError::InvalidGeometry);
                0.0
            }
        }
    }

    fn native_extents(&self, op: Op, text: &str, style: &TextStyle) -> (f64, f64) {
        match self.native_value(op, text, style, None, WrapMode::SvgLike) {
            Some(Value::HorizontalExtents { left, right }) => (left, right),
            None => (0.0, 0.0),
            Some(_) => {
                self.record(CanonicalError::InvalidGeometry);
                (0.0, 0.0)
            }
        }
    }

    fn native_metrics(
        &self,
        op: Op,
        text: &str,
        style: &TextStyle,
        width: Option<f64>,
        mode: WrapMode,
    ) -> TextMetrics {
        match self.native_value(op, text, style, width, mode) {
            Some(Value::Metrics(value)) => value,
            None => TextMetrics {
                width: 0.0,
                height: 0.0,
                line_count: 0,
            },
            Some(_) => {
                self.record(CanonicalError::InvalidGeometry);
                TextMetrics {
                    width: 0.0,
                    height: 0.0,
                    line_count: 0,
                }
            }
        }
    }
}

macro_rules! length {
    ($name:ident, $operation:ident) => {
        fn $name(&self, text: &str, style: &TextStyle) -> f64 {
            self.native_length(Op::$operation, text, style, WrapMode::SvgLike)
        }
    };
}
macro_rules! offset {
    ($name:ident, $operation:ident) => {
        fn $name(&self, text: &str, style: &TextStyle) -> Option<f64> {
            Some(self.native_length(Op::$operation, text, style, WrapMode::SvgLike))
        }
    };
}
macro_rules! extents {
    ($name:ident, $operation:ident) => {
        fn $name(&self, text: &str, style: &TextStyle) -> (f64, f64) {
            self.native_extents(Op::$operation, text, style)
        }
    };
}

impl TextMeasurer for WindowsText {
    fn cancellation_requested(&self) -> bool {
        self.checkpoint().is_err()
    }
    fn measure(&self, text: &str, style: &TextStyle) -> TextMetrics {
        self.native_metrics(Op::Measure, text, style, None, WrapMode::SvgLike)
    }
    length!(measure_svg_text_computed_length_px, ComputedLength);
    length!(measure_canvas_text_width_px, CanvasMeasureTextWidth);
    extents!(measure_svg_text_bbox_x, BBoxX);
    extents!(
        measure_svg_text_bbox_x_with_ascii_overhang,
        BBoxXWithAsciiOverhang
    );
    extents!(measure_svg_title_bbox_x, TitleBBoxX);
    length!(measure_svg_simple_text_bbox_width_px, SimpleBBoxWidth);
    length!(measure_svg_raw_text_bbox_width_px, RawBBoxWidth);
    length!(measure_svg_raw_text_bbox_height_px, RawBBoxHeight);
    offset!(measure_svg_raw_text_bbox_y_offset_px, RawBBoxYOffset);
    offset!(
        measure_svg_raw_text_middle_bbox_y_offset_px,
        RawMiddleBBoxYOffset
    );
    offset!(
        measure_svg_raw_text_hanging_bbox_y_offset_px,
        RawHangingBBoxYOffset
    );
    length!(
        measure_svg_text_bounding_client_rect_width_px,
        BoundingClientRectWidth
    );
    length!(measure_svg_tspan_text_bbox_width_px, TspanBBoxWidth);
    fn measure_svg_preserved_tspan_text_bbox_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.native_length(Op::TspanBBoxWidth, text, style, WrapMode::HtmlLike)
    }
    length!(measure_svg_tspan_text_bbox_height_px, TspanBBoxHeight);
    length!(
        measure_svg_create_text_bbox_y_offset_px,
        CreateTextBBoxYOffset
    );
    length!(
        measure_svg_create_text_middle_bbox_y_offset_px,
        CreateTextMiddleBBoxYOffset
    );
    length!(
        measure_svg_simple_text_bbox_width_for_wrap_px,
        WrapProbeBBoxWidth
    );
    length!(measure_svg_simple_text_bbox_height_px, SimpleBBoxHeight);
    fn measure_mermaid_calculate_text_dimensions(
        &self,
        text: &str,
        style: &TextStyle,
    ) -> TextMetrics {
        self.native_metrics(
            Op::MermaidCalculateTextDimensions,
            text,
            style,
            None,
            WrapMode::SvgLike,
        )
    }
    fn measure_html_line_width_px(&self, text: &str, style: &TextStyle) -> f64 {
        self.native_metrics(Op::Wrapped, text, style, None, WrapMode::HtmlLike)
            .width
    }
    fn measure_wrapped(
        &self,
        text: &str,
        style: &TextStyle,
        width: Option<f64>,
        mode: WrapMode,
    ) -> TextMetrics {
        self.native_metrics(Op::Wrapped, text, style, width, mode)
    }
    fn measure_wrapped_with_raw_width(
        &self,
        text: &str,
        style: &TextStyle,
        width: Option<f64>,
        mode: WrapMode,
    ) -> (TextMetrics, Option<f64>) {
        match self.native_value(Op::WrappedWithRawWidth, text, style, width, mode) {
            Some(Value::WrappedWithRawWidth { metrics, raw_width }) => (metrics, raw_width),
            None => (
                TextMetrics {
                    width: 0.0,
                    height: 0.0,
                    line_count: 0,
                },
                None,
            ),
            Some(_) => {
                self.record(CanonicalError::InvalidGeometry);
                (
                    TextMetrics {
                        width: 0.0,
                        height: 0.0,
                        line_count: 0,
                    },
                    None,
                )
            }
        }
    }
}
