// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Original conservative retention arithmetic, evaluated with owned continuation frames.
use super::{path_bytes, path_owner_bytes, LocalPayload};
use crate::tree::owner;
use crate::{Group, Node, ProcessingContext};
use markdown_svg_budget::{BudgetVec, Error, ResourceKind};
use std::{mem::size_of, sync::Arc};

#[derive(Clone, Copy)]
enum Frame<'a> {
    Group(&'a Group, usize),
    GroupNext {
        group: &'a Group,
        depth: usize,
        phase: u8,
        index: usize,
        sum: usize,
        add: bool,
    },
    Node(&'a Node, usize),
    PathFillDone(&'a crate::Path, usize, usize),
    Paint(&'a crate::Paint, usize),
    Clip(&'a crate::ClipPath, usize),
    ClipRootDone(&'a crate::ClipPath, usize, usize),
    Mask(&'a crate::Mask, usize),
    MaskRootDone(&'a crate::Mask, usize, usize),
    Filter(&'a crate::filter::Filter, usize),
    FilterNext {
        filter: &'a crate::filter::Filter,
        depth: usize,
        index: usize,
        sum: usize,
        add: bool,
    },
    Primitive(&'a crate::filter::Kind, usize),
    Merge {
        inputs: &'a [crate::filter::Input],
        index: usize,
        sum: usize,
    },
    Image(&'a crate::Image),
    ImageRootDone(&'a crate::Image, usize),
    Add(usize),
}

enum Frames<'a> {
    Plain(Vec<Frame<'a>>),
    Paid(BudgetVec<Frame<'a>>),
}
impl<'a> Frames<'a> {
    fn new(p: &ProcessingContext) -> Self {
        match p.resources() {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::Plain(Vec::new()),
        }
    }
    fn push(&mut self, value: Frame<'a>, p: &ProcessingContext) -> Result<(), Error> {
        owner::check(p, 1)?;
        match self {
            Self::Plain(v) => {
                v.push(value);
                Ok(())
            }
            Self::Paid(v) => v.try_push(value),
        }
    }
    fn pop(&mut self) -> Option<Frame<'a>> {
        match self {
            Self::Plain(v) => v.pop(),
            Self::Paid(v) => v.pop(),
        }
    }
    fn is_empty(&self) -> bool {
        match self {
            Self::Plain(v) => v.is_empty(),
            Self::Paid(v) => v.is_empty(),
        }
    }
}

pub(super) fn payload_bytes(
    payload: &LocalPayload,
    p: &ProcessingContext,
) -> Result<Option<usize>, Error> {
    owner::check(p, 1)?;
    match payload {
        LocalPayload::Ordinary(Some(path)) => Ok(path_bytes(path)),
        LocalPayload::Ordinary(None) | LocalPayload::Synthetic { .. } => Ok(Some(0)),
        LocalPayload::Bitmap(image) => estimate(Frame::Image(&image.image), p),
        LocalPayload::Colr(group) => estimate(Frame::Group(group, 0), p),
        LocalPayload::Svg(node) => estimate(Frame::Node(node, 0), p),
    }
}

fn estimate(root: Frame<'_>, p: &ProcessingContext) -> Result<Option<usize>, Error> {
    let mut frames = Frames::new(p);
    let candidate = (|| {
        frames.push(root, p)?;
        walk(&mut frames, p)
    })()
    .map_err(|e| p.record_resource(e));
    // The operation error must enter the original slot before the actual stack retires.
    drop(frames);
    owner::check(p, 0)?;
    candidate
}

fn walk(frames: &mut Frames<'_>, p: &ProcessingContext) -> Result<Option<usize>, Error> {
    macro_rules! some {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => return Ok(None),
            }
        };
    }
    let mut value = 0usize;
    // Popping moves a Copy frame out; no borrow of a movable Vec survives a push.
    while !frames.is_empty() {
        owner::check(p, 1)?;
        let Some(frame) = frames.pop() else {
            break;
        };
        match frame {
            Frame::Add(left) => value = some!(left.checked_add(value)),
            Frame::Group(group, depth) => {
                if depth > 256 {
                    return Ok(None);
                }
                let sum = some!(size_of::<Group>()
                    .checked_add(group.id.capacity())
                    .and_then(|v| v
                        .checked_add(group.children.capacity().checked_mul(size_of::<Node>())?))
                    .and_then(|v| v.checked_add(
                        group
                            .filters
                            .capacity()
                            .checked_mul(size_of::<Arc<crate::filter::Filter>>())?
                    )));
                frames.push(
                    Frame::GroupNext {
                        group,
                        depth,
                        phase: 0,
                        index: 0,
                        sum,
                        add: false,
                    },
                    p,
                )?;
            }
            Frame::GroupNext {
                group,
                depth,
                phase,
                index,
                mut sum,
                add,
            } => {
                if add {
                    sum = some!(sum.checked_add(value));
                }
                match phase {
                    0 if index < group.children.len() => {
                        frames.push(
                            Frame::GroupNext {
                                group,
                                depth,
                                phase,
                                index: index + 1,
                                sum,
                                add: true,
                            },
                            p,
                        )?;
                        frames.push(Frame::Node(&group.children[index], depth + 1), p)?;
                    }
                    0 => frames.push(
                        Frame::GroupNext {
                            group,
                            depth,
                            phase: 1,
                            index: 0,
                            sum,
                            add: false,
                        },
                        p,
                    )?,
                    1 => {
                        frames.push(
                            Frame::GroupNext {
                                group,
                                depth,
                                phase: 2,
                                index: 0,
                                sum,
                                add: group.clip_path.is_some(),
                            },
                            p,
                        )?;
                        if let Some(clip) = &group.clip_path {
                            frames.push(Frame::Clip(clip, depth + 1), p)?;
                        }
                    }
                    2 => {
                        frames.push(
                            Frame::GroupNext {
                                group,
                                depth,
                                phase: 3,
                                index: 0,
                                sum,
                                add: group.mask.is_some(),
                            },
                            p,
                        )?;
                        if let Some(mask) = &group.mask {
                            frames.push(Frame::Mask(mask, depth + 1), p)?;
                        }
                    }
                    3 if index < group.filters.len() => {
                        frames.push(
                            Frame::GroupNext {
                                group,
                                depth,
                                phase,
                                index: index + 1,
                                sum,
                                add: true,
                            },
                            p,
                        )?;
                        frames.push(Frame::Filter(&group.filters[index], depth + 1), p)?;
                    }
                    _ => value = sum,
                }
            }
            Frame::Node(node, depth) => {
                if depth > 256 {
                    return Ok(None);
                }
                match node {
                    Node::Group(group) => frames.push(Frame::Group(group, depth + 1), p)?,
                    Node::Path(path) => {
                        let sum = some!(size_of::<crate::Path>()
                            .checked_add(path.id.capacity())
                            .and_then(|v| v.checked_add(path_owner_bytes(&path.data)?))
                            .and_then(|v| v.checked_add(
                                path.stroke
                                    .as_ref()
                                    .and_then(|s| s.dasharray.as_ref())
                                    .map_or(Some(0), |v| v
                                        .retained_unpaid_capacity()
                                        .checked_mul(size_of::<f32>()))?
                            )));
                        frames.push(Frame::PathFillDone(path, depth, sum), p)?;
                        value = 0;
                        if let Some(fill) = &path.fill {
                            frames.push(Frame::Paint(&fill.paint, depth + 1), p)?;
                        }
                    }
                    Node::Image(image) => frames.push(Frame::Image(image), p)?,
                    Node::Text(_) => return Ok(None),
                }
            }
            Frame::PathFillDone(path, depth, sum) => {
                let sum = some!(sum.checked_add(value));
                frames.push(Frame::Add(sum), p)?;
                value = 0;
                if let Some(stroke) = &path.stroke {
                    frames.push(Frame::Paint(&stroke.paint, depth + 1), p)?;
                }
            }
            Frame::Paint(paint, depth) => match paint {
                crate::Paint::Color(_) => value = 0,
                crate::Paint::LinearGradient(v) => {
                    value = some!(size_of::<crate::LinearGradient>()
                        .checked_add(some!(id_bytes(v.id())))
                        .and_then(|n| n.checked_add(
                            v.stops.capacity().checked_mul(size_of::<crate::Stop>())?
                        )))
                }
                crate::Paint::RadialGradient(v) => {
                    value = some!(size_of::<crate::RadialGradient>()
                        .checked_add(some!(id_bytes(v.id())))
                        .and_then(|n| n.checked_add(
                            v.stops.capacity().checked_mul(size_of::<crate::Stop>())?
                        )))
                }
                crate::Paint::Pattern(v) => {
                    let sum =
                        some!(size_of::<crate::Pattern>().checked_add(some!(id_bytes(v.id()))));
                    frames.push(Frame::Add(sum), p)?;
                    frames.push(Frame::Group(v.root(), depth + 1), p)?;
                }
            },
            Frame::Clip(clip, depth) => {
                if depth > 256 {
                    return Ok(None);
                }
                let sum =
                    some!(size_of::<crate::ClipPath>().checked_add(some!(id_bytes(clip.id()))));
                frames.push(Frame::ClipRootDone(clip, depth, sum), p)?;
                frames.push(Frame::Group(clip.root(), depth + 1), p)?;
            }
            Frame::ClipRootDone(clip, depth, sum) => {
                let sum = some!(sum.checked_add(value));
                frames.push(Frame::Add(sum), p)?;
                value = 0;
                if let Some(child) = clip.clip_path() {
                    frames.push(Frame::Clip(child, depth + 1), p)?;
                }
            }
            Frame::Mask(mask, depth) => {
                if depth > 256 {
                    return Ok(None);
                }
                let sum = some!(size_of::<crate::Mask>().checked_add(some!(id_bytes(mask.id()))));
                frames.push(Frame::MaskRootDone(mask, depth, sum), p)?;
                frames.push(Frame::Group(mask.root(), depth + 1), p)?;
            }
            Frame::MaskRootDone(mask, depth, sum) => {
                let sum = some!(sum.checked_add(value));
                frames.push(Frame::Add(sum), p)?;
                value = 0;
                if let Some(child) = mask.mask() {
                    frames.push(Frame::Mask(child, depth + 1), p)?;
                }
            }
            Frame::Filter(filter, depth) => {
                if depth > 256 {
                    return Ok(None);
                }
                let sum = some!(size_of::<crate::filter::Filter>()
                    .checked_add(some!(id_bytes(filter.id())))
                    .and_then(|v| v.checked_add(
                        filter
                            .primitives
                            .capacity()
                            .checked_mul(size_of::<crate::filter::Primitive>())?
                    )));
                frames.push(
                    Frame::FilterNext {
                        filter,
                        depth,
                        index: 0,
                        sum,
                        add: false,
                    },
                    p,
                )?;
            }
            Frame::FilterNext {
                filter,
                depth,
                index,
                mut sum,
                add,
            } => {
                if add {
                    sum = some!(sum.checked_add(value));
                }
                if let Some(primitive) = filter.primitives.get(index) {
                    sum = some!(sum.checked_add(primitive.result.capacity()));
                    frames.push(
                        Frame::FilterNext {
                            filter,
                            depth,
                            index: index + 1,
                            sum,
                            add: true,
                        },
                        p,
                    )?;
                    frames.push(Frame::Primitive(&primitive.kind, depth), p)?;
                } else {
                    value = sum;
                }
            }
            Frame::Primitive(kind, depth) => {
                use crate::filter::{ColorMatrixKind, Kind};
                value = match kind {
                    Kind::Blend(v) => {
                        some!(input_bytes(&v.input1).checked_add(input_bytes(&v.input2)))
                    }
                    Kind::Composite(v) => {
                        some!(input_bytes(&v.input1).checked_add(input_bytes(&v.input2)))
                    }
                    Kind::DisplacementMap(v) => {
                        some!(input_bytes(&v.input1).checked_add(input_bytes(&v.input2)))
                    }
                    Kind::ColorMatrix(v) => {
                        some!(input_bytes(&v.input).checked_add(match &v.kind {
                            ColorMatrixKind::Matrix(m) =>
                                some!(m.retained_unpaid_capacity().checked_mul(size_of::<f32>())),
                            _ => 0,
                        }))
                    }
                    Kind::ComponentTransfer(v) => some!(input_bytes(&v.input)
                        .checked_add(some!(transfer_bytes(&v.func_r)))
                        .and_then(|n| n.checked_add(transfer_bytes(&v.func_g)?))
                        .and_then(|n| n.checked_add(transfer_bytes(&v.func_b)?))
                        .and_then(|n| n.checked_add(transfer_bytes(&v.func_a)?))),
                    Kind::ConvolveMatrix(v) => some!(input_bytes(&v.input).checked_add(some!(v
                        .matrix
                        .data
                        .retained_unpaid_capacity()
                        .checked_mul(size_of::<f32>())))),
                    Kind::DiffuseLighting(v) => input_bytes(&v.input),
                    Kind::DropShadow(v) => input_bytes(&v.input),
                    Kind::GaussianBlur(v) => input_bytes(&v.input),
                    Kind::Morphology(v) => input_bytes(&v.input),
                    Kind::Offset(v) => input_bytes(&v.input),
                    Kind::SpecularLighting(v) => input_bytes(&v.input),
                    Kind::Tile(v) => input_bytes(&v.input),
                    Kind::Flood(_) | Kind::Turbulence(_) => 0,
                    Kind::Image(v) => {
                        frames.push(Frame::Group(v.root(), depth + 1), p)?;
                        continue;
                    }
                    Kind::Merge(v) => {
                        let sum = some!(v
                            .inputs
                            .capacity()
                            .checked_mul(size_of::<crate::filter::Input>()));
                        frames.push(
                            Frame::Merge {
                                inputs: &v.inputs,
                                index: 0,
                                sum,
                            },
                            p,
                        )?;
                        continue;
                    }
                };
            }
            Frame::Merge { inputs, index, sum } => {
                if let Some(input) = inputs.get(index) {
                    let sum = some!(sum.checked_add(input_bytes(input)));
                    frames.push(
                        Frame::Merge {
                            inputs,
                            index: index + 1,
                            sum,
                        },
                        p,
                    )?;
                } else {
                    value = sum;
                }
            }
            Frame::Image(image) => match image.kind() {
                crate::ImageKind::JPEG(v)
                | crate::ImageKind::PNG(v)
                | crate::ImageKind::GIF(v)
                | crate::ImageKind::WEBP(v) => {
                    value = some!(size_of::<crate::Image>()
                        .checked_add(image.id.capacity())
                        .and_then(|n| n.checked_add(v.retained_unpaid_capacity())));
                }
                crate::ImageKind::SVG(tree) => {
                    let vectors = some!(tree
                        .linear_gradients
                        .capacity()
                        .checked_add(tree.radial_gradients.capacity())
                        .and_then(|n| n.checked_add(tree.patterns.capacity()))
                        .and_then(|n| n.checked_add(tree.clip_paths.capacity()))
                        .and_then(|n| n.checked_add(tree.masks.capacity()))
                        .and_then(|n| n.checked_add(tree.filters.capacity())));
                    frames.push(Frame::ImageRootDone(image, vectors), p)?;
                    frames.push(Frame::Group(tree.root(), 0), p)?;
                }
            },
            Frame::ImageRootDone(image, vectors) => {
                let data = some!(size_of::<crate::Tree>().checked_add(value).and_then(
                    |n| n.checked_add(vectors.checked_mul(size_of::<Arc<crate::Group>>())?)
                ));
                value = some!(size_of::<crate::Image>()
                    .checked_add(image.id.capacity())
                    .and_then(|n| n.checked_add(data)));
            }
        }
    }
    Ok(Some(value))
}

fn id_bytes(id: &str) -> Option<usize> {
    id.len().checked_mul(2)?.checked_add(8)
}
#[cfg(test)]
pub(super) fn test_node_bytes(node: &Node, depth: usize) -> Option<usize> {
    estimate(Frame::Node(node, depth), &ProcessingContext::default()).unwrap()
}
fn input_bytes(input: &crate::filter::Input) -> usize {
    match input {
        crate::filter::Input::Reference(value) => value.capacity(),
        _ => 0,
    }
}
fn transfer_bytes(value: &crate::filter::TransferFunction) -> Option<usize> {
    match value {
        crate::filter::TransferFunction::Table(v)
        | crate::filter::TransferFunction::Discrete(v) => {
            v.retained_unpaid_capacity().checked_mul(size_of::<f32>())
        }
        _ => Some(0),
    }
}

#[cfg(test)]
#[path = "representation_retention_tests.rs"]
mod tests;
