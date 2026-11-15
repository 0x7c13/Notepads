// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::tree::owner::{Origin, PolicyValue, TreeString};
use std::sync::Arc;

use svgtypes::{AspectRatio, Length};

use super::svgtree::{AId, SvgNode};
use super::{converter, OptionLog, Options};
use crate::{
    ClipPath, Group, Image, ImageKind, ImageRendering, Node, NonZeroRect, Size, Transform, Tree,
    Visibility,
};
#[cfg(test)]
#[path = "image_resource_tests.rs"]
mod resource_tests;

/// A shorthand for [ImageHrefResolver]'s data function.
/// One resolver owner or a borrowed view of the same callback.
pub enum ImageHrefDataResolverFn<'a> {
    /// Callback owned by the external configuration.
    Owned(
        Box<
            dyn Fn(&str, crate::EncodedImageData, &Options) -> Option<ImageKind> + Send + Sync + 'a,
        >,
    ),
    /// Callback borrowed without allocating a forwarding closure.
    Borrowed(
        &'a (dyn Fn(&str, crate::EncodedImageData, &Options) -> Option<ImageKind>
                 + Send
                 + Sync
                 + 'a),
    ),
}
impl<'a> std::ops::Deref for ImageHrefDataResolverFn<'a> {
    type Target =
        dyn Fn(&str, crate::EncodedImageData, &Options) -> Option<ImageKind> + Send + Sync + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => *v,
        }
    }
}
impl ImageHrefDataResolverFn<'_> {
    pub(crate) fn borrowed(&self) -> ImageHrefDataResolverFn<'_> {
        ImageHrefDataResolverFn::Borrowed(&**self)
    }
}

/// A shorthand for [ImageHrefResolver]'s string function.
/// One resolver owner or a borrowed view of the same callback.
pub enum ImageHrefStringResolverFn<'a> {
    /// Callback owned by the external configuration.
    Owned(Box<dyn Fn(&str, &Options) -> Option<ImageKind> + Send + Sync + 'a>),
    /// Callback borrowed without allocating a forwarding closure.
    Borrowed(&'a (dyn Fn(&str, &Options) -> Option<ImageKind> + Send + Sync + 'a)),
}
impl<'a> std::ops::Deref for ImageHrefStringResolverFn<'a> {
    type Target = dyn Fn(&str, &Options) -> Option<ImageKind> + Send + Sync + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => *v,
        }
    }
}
impl ImageHrefStringResolverFn<'_> {
    pub(crate) fn borrowed(&self) -> ImageHrefStringResolverFn<'_> {
        ImageHrefStringResolverFn::Borrowed(&**self)
    }
}

/// An `xlink:href` resolver for `<image>` elements.
///
/// This type supplies the request's controlled resource policy.
/// The isolated prototype default rejects external paths and URLs without IO.
pub struct ImageHrefResolver<'a> {
    /// Resolver function that will be used when `xlink:href` contains a
    /// [Data URL](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs).
    ///
    /// A function would be called with mime, decoded base64 data and parsing options.
    pub resolve_data: ImageHrefDataResolverFn<'a>,

    /// Resolver function that will be used to handle an arbitrary string in `xlink:href`.
    pub resolve_string: ImageHrefStringResolverFn<'a>,
}

impl Default for ImageHrefResolver<'_> {
    fn default() -> Self {
        ImageHrefResolver {
            resolve_data: ImageHrefResolver::default_data_resolver(),
            resolve_string: ImageHrefResolver::default_string_resolver(),
        }
    }
}

impl ImageHrefResolver<'_> {
    pub(crate) fn ignored_string_resolver() -> ImageHrefStringResolverFn<'static> {
        fn ignore(_: &str, _: &Options) -> Option<ImageKind> {
            None
        }
        ImageHrefStringResolverFn::Borrowed(&ignore)
    }
    /// Creates a default
    /// [Data URL](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Data_URIs)
    /// resolver closure.
    ///
    /// base64 encoded data is already decoded.
    ///
    /// The default implementation would try to load JPEG, PNG, GIF, WebP, SVG and SVGZ types.
    /// Note that it will simply match the `mime` or data's magic.
    /// The actual images would not be decoded. It's up to the renderer.
    pub fn default_data_resolver() -> ImageHrefDataResolverFn<'static> {
        ImageHrefDataResolverFn::Owned(Box::new(
            move |mime: &str, data: crate::EncodedImageData, opts: &Options| match mime {
                "image/jpg" | "image/jpeg" => Some(ImageKind::JPEG(data)),
                "image/png" => Some(ImageKind::PNG(data)),
                "image/gif" => Some(ImageKind::GIF(data)),
                "image/webp" => Some(ImageKind::WEBP(data)),
                "image/svg+xml" => load_sub_svg(&data, opts),
                "text/plain" => match get_image_data_format(&data) {
                    Some(ImageFormat::JPEG) => Some(ImageKind::JPEG(data)),
                    Some(ImageFormat::PNG) => Some(ImageKind::PNG(data)),
                    Some(ImageFormat::GIF) => Some(ImageKind::GIF(data)),
                    Some(ImageFormat::WEBP) => Some(ImageKind::WEBP(data)),
                    _ => load_sub_svg(&data, opts),
                },
                _ => None,
            },
        ))
    }

    /// Creates a default string resolver.
    ///
    /// The controlled prototype default rejects every external string.
    /// It performs no filesystem or network access.
    pub fn default_string_resolver() -> ImageHrefStringResolverFn<'static> {
        // Controlled prototype default: external bytes are never read here.
        ImageHrefStringResolverFn::Owned(Box::new(|_, _| None))
    }
}

impl std::fmt::Debug for ImageHrefResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ImageHrefResolver { .. }")
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ImageFormat {
    PNG,
    JPEG,
    GIF,
    WEBP,
}

pub(crate) fn convert(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
    parent: &mut Group,
) -> Option<()> {
    if let Err(error) = super::shapes::checkpoint(&cache.processing, 0) {
        cache.processing.record_resource(error);
        return None;
    }
    let href = node
        .try_attribute(AId::Href)
        .log_none(|| log::warn!("Image lacks the 'xlink:href' attribute. Skipped."))?;

    let kind = get_href_data(href, state)?;

    let visibility: Visibility = node.find_attribute(AId::Visibility).unwrap_or_default();
    let visible = visibility == Visibility::Visible;

    let rendering_mode = node
        .find_attribute(AId::ImageRendering)
        .unwrap_or(state.opt.image_rendering);

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let id = TreeString::try_from_str(
        if state.parent_markers.is_empty() {
            node.element_id()
        } else {
            ""
        },
        &state.opt.processing,
    )
    .ok()?;

    let actual_size = kind.actual_size(&state.opt.processing)?;

    let x = node.convert_user_length(AId::X, state, Length::zero());
    let y = node.convert_user_length(AId::Y, state, Length::zero());
    let mut width = node.convert_user_length(
        AId::Width,
        state,
        Length::new_number(actual_size.width() as f64),
    );
    let mut height = node.convert_user_length(
        AId::Height,
        state,
        Length::new_number(actual_size.height() as f64),
    );

    match (
        node.attribute::<Length>(AId::Width),
        node.attribute::<Length>(AId::Height),
    ) {
        (Some(_), None) => {
            // Only width was defined, so we need to scale height accordingly.
            height = actual_size.height() * (width / actual_size.width());
        }
        (None, Some(_)) => {
            // Only height was defined, so we need to scale width accordingly.
            width = actual_size.width() * (height / actual_size.height());
        }
        _ => {}
    };

    let aspect: AspectRatio = node.attribute(AId::PreserveAspectRatio).unwrap_or_default();

    let rect = NonZeroRect::from_xywh(x, y, width, height);
    let rect = rect.log_none(|| log::warn!("Image has an invalid size. Skipped."))?;

    convert_inner(
        kind,
        id,
        visible,
        rendering_mode,
        aspect,
        actual_size,
        rect,
        cache,
        parent,
    )
}

pub(crate) fn convert_inner(
    kind: ImageKind,
    id: TreeString,
    visible: bool,
    rendering_mode: ImageRendering,
    aspect: AspectRatio,
    actual_size: Size,
    rect: NonZeroRect,
    cache: &mut converter::Cache,
    parent: &mut Group,
) -> Option<()> {
    if let Err(error) = super::shapes::checkpoint(&cache.processing, 0) {
        cache.processing.record_resource(error);
        return None;
    }
    let aligned_size = fit_view_box(actual_size, rect, aspect);
    let (aligned_x, aligned_y) = crate::aligned_pos(
        aspect.align,
        rect.x(),
        rect.y(),
        rect.width() - aligned_size.width(),
        rect.height() - aligned_size.height(),
    );
    let view_box = aligned_size.to_non_zero_rect(aligned_x, aligned_y);

    let image_ts = Transform::from_row(
        view_box.width() / actual_size.width(),
        0.0,
        0.0,
        view_box.height() / actual_size.height(),
        view_box.x(),
        view_box.y(),
    );

    let abs_transform = parent.abs_transform.pre_concat(image_ts);
    let abs_bounding_box = view_box.transform(parent.abs_transform)?;

    let mut g = Group::empty_in(&cache.processing);
    g.id = id;
    let image = crate::scene::boxed(&cache.processing, crate::scene::SceneCost::NODE, || Image {
        origin: Origin::new(&cache.processing),
        id: TreeString::new(&cache.processing),
        visible,
        size: actual_size,
        rendering_mode,
        kind,
        abs_transform,
        abs_bounding_box,
    })
    .ok()?;
    g.children
        .try_push(Node::Image(image), &cache.processing)
        .ok()?;
    g.transform = image_ts;
    g.abs_transform = abs_transform;
    g.calculate_bounding_boxes();

    if aspect.slice {
        // Image slice acts like a rectangular clip.
        let mut path = match super::shapes::rect_to_node_path(rect.to_rect(), &cache.processing) {
            Ok(Some(path)) => path,
            Ok(None) => return None,
            Err(error) => {
                cache.processing.record_resource(error);
                return None;
            }
        };
        path.fill = Some(crate::Fill::default());

        let root =
            crate::scene::root(&cache.processing, || Group::empty_in(&cache.processing)).ok()?;
        let mut clip = ClipPath::empty(cache.gen_clip_path_id().ok()?);
        clip.root = root;
        let cost = crate::scene::SceneCost::path(&path);
        let path = crate::scene::boxed_owned(&cache.processing, cost, path).ok()?;
        clip.root
            .children
            .try_push(Node::Path(path), &cache.processing)
            .ok()?;
        clip.root.calculate_bounding_boxes();

        // Clip path should not be affected by the image viewbox transform.
        // The final structure should look like:
        // <g clip-path="url(#clipPath1)">
        //     <g transform="matrix(1 0 0 1 10 20)">
        //         <image/>
        //     </g>
        // </g>

        let mut g2 = Group::empty_in(&cache.processing);
        std::mem::swap(&mut g.id, &mut g2.id);
        g2.abs_transform = parent.abs_transform;
        g2.clip_path = Some(
            crate::SharedOwner::try_new(
                clip,
                cache.processing.resources(),
                markdown_svg_budget::ResourceKind::Scratch,
            )
            .ok()?,
        );
        let group =
            crate::scene::boxed_owned(&cache.processing, crate::scene::SceneCost::NODE, g).ok()?;
        g2.children
            .try_push(Node::Group(group), &cache.processing)
            .ok()?;
        g2.calculate_bounding_boxes();

        // With `slice`, the image is scaled to *cover* the viewport and the
        // overflow is cut away by the rectangular clip above. The bounding box
        // of the element is therefore its `rect` (the viewport), not the
        // larger scaled-to-cover image that `calculate_bounding_boxes` derives
        // from the children. Without this correction an `objectBoundingBox`
        // clip/mask/filter applied to the `<image>` would be resolved against
        // the oversized image. See https://github.com/linebender/resvg/issues/1034
        let bbox = rect.to_rect();
        g2.bounding_box = bbox;
        g2.stroke_bounding_box = bbox;
        if let Some(abs) = bbox.transform(g2.abs_transform) {
            g2.abs_bounding_box = abs;
            g2.abs_stroke_bounding_box = abs;
        }

        let group =
            crate::scene::boxed_owned(&cache.processing, crate::scene::SceneCost::NODE, g2).ok()?;
        parent
            .children
            .try_push(Node::Group(group), &cache.processing)
            .ok()?;
    } else {
        let group =
            crate::scene::boxed_owned(&cache.processing, crate::scene::SceneCost::NODE, g).ok()?;
        parent
            .children
            .try_push(Node::Group(group), &cache.processing)
            .ok()?;
    }

    Some(())
}

pub(crate) fn get_href_data(href: &str, state: &converter::State) -> Option<ImageKind> {
    if state.opt.processing.failed() {
        return None;
    }
    if let Some(resources) = state.opt.processing.resources() {
        return get_href_data_controlled(href, state.opt, resources);
    }
    if let Ok(url) = data_url::DataUrl::process(href) {
        let (data, _) = match url.decode_to_vec() {
            Ok(data) => data,
            Err(_) => {
                state
                    .opt
                    .processing
                    .fail(crate::ProcessingFailure::MalformedImageData);
                return None;
            }
        };

        let mime = format!(
            "{}/{}",
            url.mime_type().type_.as_str(),
            url.mime_type().subtype.as_str()
        );

        let image =
            (state.opt.image_href_resolver.resolve_data)(&mime, Arc::new(data).into(), state.opt);
        if image.is_none() {
            state
                .opt
                .processing
                .fail(crate::ProcessingFailure::UnsupportedImageData);
        }
        image
    } else {
        if href.starts_with("data:") {
            state
                .opt
                .processing
                .fail(crate::ProcessingFailure::MalformedImageData);
            return None;
        }
        let image = (state.opt.image_href_resolver.resolve_string)(href, state.opt);
        if image.is_none() {
            state
                .opt
                .processing
                .fail(crate::ProcessingFailure::ExternalResource);
        }
        image
    }
}

fn record_url_error(opt: &Options, error: data_url::controlled::Error) {
    match error {
        data_url::controlled::Error::Resource(error) => {
            opt.processing.record_resource(error);
        }
        data_url::controlled::Error::InvalidBase64(_) | data_url::controlled::Error::Syntax(_) => {
            opt.processing
                .fail(crate::ProcessingFailure::MalformedImageData);
        }
    }
}

fn callback_image_is_admitted(
    image: &ImageKind,
    opt: &Options,
    resources: &markdown_svg_budget::ResourcesRef,
) -> bool {
    match image {
        ImageKind::JPEG(data)
        | ImageKind::PNG(data)
        | ImageKind::GIF(data)
        | ImageKind::WEBP(data) => data.is_admitted_for(resources),
        ImageKind::SVG(tree) => {
            !opt.source_kind.font()
                && matches!(
                    tree.source_kind,
                    super::source_kind::SourceKind::GeneralImage
                )
                && Arc::ptr_eq(tree.processing_context(), &opt.processing)
                && tree
                    .processing_context()
                    .resources()
                    .is_some_and(|owner| owner.is_same_request(resources))
        }
    }
}

fn validate_callback_image(
    image: Option<ImageKind>,
    opt: &Options,
    resources: &markdown_svg_budget::ResourcesRef,
    none_kind: crate::ProcessingFailure,
) -> Option<ImageKind> {
    match image {
        Some(image) => {
            if image.validate_policy(&opt.processing).is_err() {
                drop(image);
                let _ = crate::tree::owner::check(&opt.processing, 0);
                return None;
            }
            if callback_image_is_admitted(&image, opt, resources) {
                return Some(image);
            }
            opt.processing
                .record_resource(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                ));
            drop(image);
            None
        }
        None => {
            opt.processing.fail(none_kind);
            None
        }
    }
}

fn get_href_data_controlled(
    href: &str,
    opt: &Options,
    resources: &markdown_svg_budget::ResourcesRef,
) -> Option<ImageKind> {
    use markdown_svg_budget::{BudgetVec, ResourceKind};
    let url = match data_url::controlled::DataUrl::process(href, resources.clone()) {
        Ok(url) => url,
        Err(data_url::controlled::Error::Syntax(_)) => {
            // A URL parser classification is not itself a fatal request. Keep
            // exactly the legacy lowercase prefix and resolver fallback rules.
            if href.starts_with("data:") {
                opt.processing
                    .fail(crate::ProcessingFailure::MalformedImageData);
                return None;
            }
            let image = (opt.image_href_resolver.resolve_string)(href, opt);
            let image = validate_callback_image(
                image,
                opt,
                resources,
                crate::ProcessingFailure::ExternalResource,
            );
            opt.processing.checked().ok()?;
            return image;
        }
        Err(error) => {
            record_url_error(opt, error);
            return None;
        }
    };
    let length = match url.decoded_length() {
        Ok(length) => length,
        Err(error) => {
            // The paid URL/header remains live while the original body error
            // is recorded; cleanup cannot replace this first semantic fault.
            record_url_error(opt, error);
            return None;
        }
    };
    let admission = resources
        .admit_child_encoded(length)
        .map_err(|error| opt.processing.record_resource(error))
        .ok()?;
    let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    bytes
        .try_reserve_exact(length)
        .map_err(|error| opt.processing.record_resource(error))
        .ok()?;
    if let Err(error) = url.decode_into(length, &mut bytes) {
        record_url_error(opt, error);
        return None;
    }
    let encoded = crate::EncodedImageData::try_from_paid(resources, bytes, admission)
        .map_err(|error| opt.processing.record_resource(error))
        .ok()?;
    let mime = match url.mime_essence() {
        Ok(mime) => mime,
        Err(error) => {
            record_url_error(opt, error);
            return None;
        }
    };
    // Keep this known input owner while the by-value callback runs. A None
    // callback may drop its argument, but cannot retire the last paid bytes
    // before its returned classification has been recorded by this caller.
    let image = (opt.image_href_resolver.resolve_data)(
        mime.as_str(),
        encoded.try_alias_in(&opt.processing).ok()?,
        opt,
    );
    let image = validate_callback_image(
        image,
        opt,
        resources,
        crate::ProcessingFailure::UnsupportedImageData,
    );
    drop(encoded);
    drop(mime);
    drop(url);
    opt.processing.checked().ok()?;
    image
}

/// Checks that file has a PNG, a GIF, a JPEG or a WebP magic bytes.
fn get_image_data_format(data: &[u8]) -> Option<ImageFormat> {
    match imagesize::image_type(data).ok()? {
        imagesize::ImageType::Gif => Some(ImageFormat::GIF),
        imagesize::ImageType::Jpeg => Some(ImageFormat::JPEG),
        imagesize::ImageType::Png => Some(ImageFormat::PNG),
        imagesize::ImageType::Webp => Some(ImageFormat::WEBP),
        _ => None,
    }
}

/// Tries to load the `ImageData` content as an SVG image or emits a warning and returns `None`.
pub(crate) fn load_sub_svg(data: &crate::EncodedImageData, opt: &Options) -> Option<ImageKind> {
    match Tree::from_data_nested(data, opt) {
        Ok(tree) => Some(ImageKind::SVG(tree)),
        Err(_) => {
            opt.processing
                .fail(crate::ProcessingFailure::NestedImageDecode);
            log::warn!("Failed to load nested SVG image.");
            None
        }
    }
}

/// Fits size into a viewbox.
fn fit_view_box(size: Size, rect: NonZeroRect, aspect: AspectRatio) -> Size {
    let s = rect.size();

    if aspect.align == svgtypes::Align::None {
        s
    } else if aspect.slice {
        size.expand_to(s)
    } else {
        size.scale_to(s)
    }
}

impl std::fmt::Debug for ImageHrefDataResolverFn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Owned(_) => "ImageHrefDataResolverFn::Owned(..)",
            Self::Borrowed(_) => "ImageHrefDataResolverFn::Borrowed(..)",
        })
    }
}

impl std::fmt::Debug for ImageHrefStringResolverFn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Owned(_) => "ImageHrefStringResolverFn::Owned(..)",
            Self::Borrowed(_) => "ImageHrefStringResolverFn::Borrowed(..)",
        })
    }
}
