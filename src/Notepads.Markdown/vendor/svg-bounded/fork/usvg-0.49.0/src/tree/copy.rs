// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Explicit field copies for the one request-owned semantic tree.
use super::owner::{self, PolicyValue, Result, TryCopy};
use super::*;
use crate::ProcessingContext;
macro_rules! leaves {($($t:ty),* $(,)?)=>{$(
 impl PolicyValue for $t {fn validate_policy(&self,p:&ProcessingContext)->Result<()>{owner::check(p,0)}}
 impl TryCopy for $t {fn try_copy_in(&self,p:&ProcessingContext)->Result<Self>{owner::check(p,0)?;Ok(*self)}}
)*}}
macro_rules! record {
 ($t:ty {$($(#[$attr:meta])* $field:ident),* $(,)?})=>{
 impl PolicyValue for $t {fn validate_policy(&self,p:&ProcessingContext)->Result<()> {
   owner::check(p,0)?; $($(#[$attr])* PolicyValue::validate_policy(&self.$field,p)?;)* owner::check(p,0)
 }}
 impl TryCopy for $t {fn try_copy_in(&self,p:&ProcessingContext)->Result<Self> {
   self.validate_policy(p)?;
   let result=(||Ok(Self{$($(#[$attr])* $field:TryCopy::try_copy_in(&self.$field,p)?),*}))();
   owner::finish(p,result)
 }}
 }
}
macro_rules! owned_record {
 ($t:ty,$frame:ident {$($(#[$attr:meta])* $field:ident),* $(,)?})=>{
 impl PolicyValue for $t {fn validate_policy(&self,p:&ProcessingContext)->Result<()>{super::validate::run(super::validate::Frame::$frame(self),p)}}
 impl TryCopy for $t {fn try_copy_in(&self,p:&ProcessingContext)->Result<Self>{
  self.validate_policy(p)?;
  let result=(||Ok(Self{$($(#[$attr])* $field:TryCopy::try_copy_in(&self.$field,p)?),*}))();owner::finish(p,result)
 }}
 }
}
macro_rules! owned_validate {
    ($t:ty,$frame:ident) => {
        impl PolicyValue for $t {
            fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
                super::validate::run(super::validate::Frame::$frame(self), p)
            }
        }
    };
}
leaves!(
    ViewBox,
    NonZeroF32,
    Units,
    Visibility,
    ShapeRendering,
    TextRendering,
    ImageRendering,
    BlendMode,
    SpreadMethod,
    Stop,
    StrokeMiterlimit,
    LineCap,
    LineJoin,
    FillRule,
    ContextElement,
    Color,
    MaskType,
    PaintOrder,
    FontStretch,
    FontVariation,
    FontStyle,
    DominantBaseline,
    AlignmentBaseline,
    BaselineShift,
    LengthAdjust,
    FontOpticalSizing,
    TextAnchor,
    WritingMode,
    filter::ColorInterpolation,
    filter::CompositeOperator,
    filter::EdgeMode,
    filter::ColorChannel,
    filter::Flood,
    filter::LightSource,
    filter::DistantLight,
    filter::PointLight,
    filter::SpotLight,
    filter::MorphologyOperator,
    filter::Turbulence,
    filter::TurbulenceKind,
    Transform,
    Rect,
    NonZeroRect,
    Size,
    PositiveF32,
    NonZeroPositiveF32,
    NormalizedF32,
    svgtypes::Color
);
impl<T: Copy + 'static> PolicyValue for markdown_svg_budget::FrozenList<T> {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        self.validate_policy(p.resources())
            .map_err(|e| p.record_resource(e))
    }
}
impl<T: Copy + 'static> TryCopy for markdown_svg_budget::FrozenList<T> {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        PolicyValue::validate_policy(self, p)?;
        owner::finish(p, Ok(self.clone()))
    }
}
impl PolicyValue for tiny_skia_path::Path {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        owner::check(p, 0)?;
        self.validate_policy(p.resources())
            .map_err(|e| p.record_resource(e))
    }
}
impl TryCopy for tiny_skia_path::Path {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        PolicyValue::validate_policy(self, p)?;
        match p.resources() {
            Some(r) => self
                .try_copy_in(r, markdown_svg_budget::ResourceKind::Geometry)
                .map_err(|e| p.record_resource(e)),
            None => Ok(self.clone()),
        }
    }
}
impl PolicyValue for NonEmptyString {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        self.0.validate_policy(p)
    }
}
owned_record!(
    Stroke,
    Stroke {
        paint,
        dasharray,
        dashoffset,
        miterlimit,
        opacity,
        width,
        linecap,
        linejoin,
        context_element
    }
);
owned_record!(
    Fill,
    Fill {
        paint,
        opacity,
        rule,
        context_element
    }
);
owned_record!(
    Group,
    Group {
        origin,
        id,
        transform,
        abs_transform,
        opacity,
        blend_mode,
        isolate,
        clip_path,
        is_context_element,
        mask,
        filters,
        bounding_box,
        abs_bounding_box,
        stroke_bounding_box,
        abs_stroke_bounding_box,
        layer_bounding_box,
        abs_layer_bounding_box,
        children
    }
);
owned_record!(
    Path,
    Path {
        origin,
        #[cfg(feature = "text")]
        glyph_fill_source,
        id,
        visible,
        fill,
        stroke,
        paint_order,
        rendering_mode,
        data,
        abs_transform,
        bounding_box,
        abs_bounding_box,
        stroke_bounding_box,
        abs_stroke_bounding_box
    }
);
owned_record!(
    Image,
    Image {
        origin,
        id,
        visible,
        size,
        rendering_mode,
        kind,
        abs_transform,
        abs_bounding_box
    }
);
owned_validate!(BaseGradient, BaseGradient);
owned_validate!(LinearGradient, Linear);
owned_validate!(RadialGradient, Radial);
owned_validate!(Pattern, Pattern);
owned_validate!(ClipPath, Clip);
owned_validate!(Mask, Mask);
owned_record!(
    Font,
    Font {
        families,
        style,
        stretch,
        weight,
        variations
    }
);
owned_record!(TextDecorationStyle, DecorationStyle { fill, stroke });
record!(TextDecoration {
    underline,
    overline,
    line_through
});
record!(BidiScope {
    identity,
    direction,
    mode
});
owned_record!(
    DecorationOwnerStyle,
    DecorationOwner {
        owner,
        font,
        size,
        optical,
        language,
        direction,
        scopes,
        dominant,
        alignment,
        baseline_shift
    }
);
owned_record!(
    TextSpan,
    TextSpan {
        computed_color,
        source_owner,
        source_ancestors,
        #[cfg(feature = "text")]
        length_directives,
        decoration_owners,
        decoration_sources,
        start,
        end,
        fill,
        stroke,
        paint_order,
        font,
        font_size,
        small_caps,
        apply_kerning,
        font_optical_sizing,
        language,
        direction,
        bidi_scopes,
        decoration,
        dominant_baseline,
        alignment_baseline,
        baseline_shift,
        visible,
        letter_spacing,
        word_spacing,
        text_length,
        length_adjust
    }
);
owned_record!(
    TextChunk,
    Chunk {
        source_addresses,
        x,
        y,
        anchor,
        spans,
        text_flow,
        text
    }
);
owned_record!(
    Text,
    Text {
        origin,
        id,
        rendering_mode,
        dx,
        dy,
        rotate,
        writing_mode,
        direction,
        chunks,
        abs_transform,
        bounding_box,
        abs_bounding_box,
        stroke_bounding_box,
        abs_stroke_bounding_box,
        flattened,
        #[cfg(feature = "text")]
        layouted
    }
);
owned_validate!(TextPath, TextPath);
owned_validate!(filter::Filter, Filter);
record!(filter::Primitive {
    rect,
    color_interpolation,
    result,
    kind
});
record!(filter::Blend {
    input1,
    input2,
    mode
});
record!(filter::ColorMatrix { input, kind });
record!(filter::ComponentTransfer {
    input,
    func_r,
    func_g,
    func_b,
    func_a
});
record!(filter::Composite {
    input1,
    input2,
    operator
});
record!(filter::ConvolveMatrix {
    input,
    matrix,
    divisor,
    bias,
    edge_mode,
    preserve_alpha
});
record!(filter::ConvolveMatrixData {
    target_x,
    target_y,
    columns,
    rows,
    data
});
record!(filter::DisplacementMap {
    input1,
    input2,
    scale,
    x_channel_selector,
    y_channel_selector
});
record!(filter::DropShadow {
    input,
    dx,
    dy,
    std_dev_x,
    std_dev_y,
    color,
    opacity
});
record!(filter::GaussianBlur {
    input,
    std_dev_x,
    std_dev_y
});
record!(filter::Image { root });
record!(filter::DiffuseLighting {
    input,
    surface_scale,
    diffuse_constant,
    lighting_color,
    light_source
});
record!(filter::SpecularLighting {
    input,
    surface_scale,
    specular_constant,
    specular_exponent,
    lighting_color,
    light_source
});
record!(filter::Merge { inputs });
record!(filter::Morphology {
    input,
    operator,
    radius_x,
    radius_y
});
record!(filter::Offset { input, dx, dy });
record!(filter::Tile { input });
owned_validate!(Node, Node);
impl TryCopy for Node {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        match self {
            Self::Group(v) => v.try_copy_in(p).map(Self::Group),
            Self::Path(v) => v.try_copy_in(p).map(Self::Path),
            Self::Image(v) => v.try_copy_in(p).map(Self::Image),
            Self::Text(v) => v.try_copy_in(p).map(Self::Text),
        }
    }
}
owned_validate!(ImageKind, ImageKind);
impl TryCopy for ImageKind {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        match self {
            Self::JPEG(v) => v.try_copy_in(p).map(Self::JPEG),
            Self::PNG(v) => v.try_copy_in(p).map(Self::PNG),
            Self::GIF(v) => v.try_copy_in(p).map(Self::GIF),
            Self::WEBP(v) => v.try_copy_in(p).map(Self::WEBP),
            Self::SVG(v) => v.try_copy_in(p).map(Self::SVG),
        }
    }
}
owned_validate!(Paint, Paint);
impl TryCopy for Paint {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        match self {
            Self::Color(v) => v.try_copy_in(p).map(Self::Color),
            Self::LinearGradient(v) => v.try_copy_in(p).map(Self::LinearGradient),
            Self::RadialGradient(v) => v.try_copy_in(p).map(Self::RadialGradient),
            Self::Pattern(v) => v.try_copy_in(p).map(Self::Pattern),
        }
    }
}
owned_validate!(filter::Kind, FilterKind);
impl TryCopy for filter::Kind {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        match self {
            Self::Blend(v) => v.try_copy_in(p).map(Self::Blend),
            Self::ColorMatrix(v) => v.try_copy_in(p).map(Self::ColorMatrix),
            Self::ComponentTransfer(v) => v.try_copy_in(p).map(Self::ComponentTransfer),
            Self::Composite(v) => v.try_copy_in(p).map(Self::Composite),
            Self::ConvolveMatrix(v) => v.try_copy_in(p).map(Self::ConvolveMatrix),
            Self::DiffuseLighting(v) => v.try_copy_in(p).map(Self::DiffuseLighting),
            Self::DisplacementMap(v) => v.try_copy_in(p).map(Self::DisplacementMap),
            Self::DropShadow(v) => v.try_copy_in(p).map(Self::DropShadow),
            Self::Flood(v) => v.try_copy_in(p).map(Self::Flood),
            Self::GaussianBlur(v) => v.try_copy_in(p).map(Self::GaussianBlur),
            Self::Image(v) => v.try_copy_in(p).map(Self::Image),
            Self::Merge(v) => v.try_copy_in(p).map(Self::Merge),
            Self::Morphology(v) => v.try_copy_in(p).map(Self::Morphology),
            Self::Offset(v) => v.try_copy_in(p).map(Self::Offset),
            Self::SpecularLighting(v) => v.try_copy_in(p).map(Self::SpecularLighting),
            Self::Tile(v) => v.try_copy_in(p).map(Self::Tile),
            Self::Turbulence(v) => v.try_copy_in(p).map(Self::Turbulence),
        }
    }
}
impl PolicyValue for FontFamily {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        match self {
            Self::Named(v) => PolicyValue::validate_policy(v, p),
            _ => owner::check(p, 0),
        }
    }
}
impl TryCopy for FontFamily {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok(match self {
            Self::Named(v) => Self::Named(v.try_copy_in(p)?),
            Self::Serif => Self::Serif,
            Self::SansSerif => Self::SansSerif,
            Self::Cursive => Self::Cursive,
            Self::Fantasy => Self::Fantasy,
            Self::Monospace => Self::Monospace,
        })
    }
}
owned_validate!(TextFlow, Flow);
impl TryCopy for TextFlow {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok(match self {
            Self::Path(v) => Self::Path(v.try_copy_in(p)?),
            Self::Linear => Self::Linear,
        })
    }
}
impl PolicyValue for filter::Input {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        match self {
            Self::Reference(v) => PolicyValue::validate_policy(v, p),
            _ => owner::check(p, 0),
        }
    }
}
impl TryCopy for filter::Input {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok(match self {
            Self::Reference(v) => Self::Reference(v.try_copy_in(p)?),
            Self::SourceGraphic => Self::SourceGraphic,
            Self::SourceAlpha => Self::SourceAlpha,
        })
    }
}
impl PolicyValue for filter::ColorMatrixKind {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        match self {
            Self::Matrix(v) => PolicyValue::validate_policy(v, p),
            _ => owner::check(p, 0),
        }
    }
}
impl TryCopy for filter::ColorMatrixKind {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok(match self {
            Self::Matrix(v) => Self::Matrix(TryCopy::try_copy_in(v, p)?),
            Self::Saturate(v) => Self::Saturate(*v),
            Self::HueRotate(v) => Self::HueRotate(*v),
            Self::LuminanceToAlpha => Self::LuminanceToAlpha,
        })
    }
}
impl PolicyValue for filter::TransferFunction {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<()> {
        match self {
            Self::Table(v) | Self::Discrete(v) => PolicyValue::validate_policy(v, p),
            _ => owner::check(p, 0),
        }
    }
}
impl TryCopy for filter::TransferFunction {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        Ok(match self {
            Self::Identity => Self::Identity,
            Self::Table(v) => Self::Table(TryCopy::try_copy_in(v, p)?),
            Self::Discrete(v) => Self::Discrete(TryCopy::try_copy_in(v, p)?),
            Self::Linear { slope, intercept } => Self::Linear {
                slope: *slope,
                intercept: *intercept,
            },
            Self::Gamma {
                amplitude,
                exponent,
                offset,
            } => Self::Gamma {
                amplitude: *amplitude,
                exponent: *exponent,
                offset: *offset,
            },
        })
    }
}

owned_validate!(Tree, Tree);
impl TryCopy for Tree {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self> {
        self.validate_policy(p)?;
        let result = (|| {
            Ok(Self {
                source_kind: self.source_kind,
                size: self.size,
                natural_size: self.natural_size,
                processing: self.processing.clone(),
                root: self.root.try_copy_in(p)?,
                linear_gradients: self.linear_gradients.try_copy_in(p)?,
                radial_gradients: self.radial_gradients.try_copy_in(p)?,
                patterns: self.patterns.try_copy_in(p)?,
                clip_paths: self.clip_paths.try_copy_in(p)?,
                masks: self.masks.try_copy_in(p)?,
                filters: self.filters.try_copy_in(p)?,
                #[cfg(feature = "text")]
                fontdb: self.fontdb.clone(),
                #[cfg(feature = "text")]
                font_replay: self.font_replay.clone(),
            })
        })();
        owner::finish(p, result)
    }
}

macro_rules! scene_copy_entry {($($t:ty),* $(,)?)=>{$(impl $t {
 /// Copy an admitted scene in the same policy and request; no implicit funding conversion.
 pub fn try_clone_in(&self,p:&crate::ProcessingContext)->std::result::Result<Self,markdown_svg_budget::Error>{crate::scene::clone_owned(p,self)}
})*}}
scene_copy_entry!(Tree, Group, Path, Image, Text, Node, ImageKind);

#[cfg(feature = "text")]
leaves!(crate::ReplayLengthDirective);
