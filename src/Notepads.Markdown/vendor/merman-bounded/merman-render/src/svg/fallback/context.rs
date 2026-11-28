use super::transform::{Affine, nested_viewport, root_mapping, root_viewport};
use crate::svg::pipeline::{
    checkpoint_loop, escape_xml_attr_with_checkpoints, next_svg_quoted_attr_with_checkpoints,
};
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub(super) struct SourceFrame {
    cumulative: Option<Affine>,
    viewport: Option<(f64, f64)>,
    class_tokens: Vec<String>,
}

impl SourceFrame {
    pub(super) fn from_tag<E>(
        tag: &str,
        name: &str,
        parent: Option<&Self>,
        retain_classes: bool,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Self, E> {
        let mut viewport = parent.and_then(|frame| frame.viewport);
        // Validate the actual CTM, including the root. Emission keeps the original
        // ancestry and never applies this cumulative matrix a second time.
        let cumulative = if parent.is_none() {
            viewport = root_viewport(tag, checkpoint)?;
            Affine::from_tag(tag, checkpoint)?
                .zip(root_mapping(tag, checkpoint)?)
                .and_then(|(transform, mapping)| transform.then_local(mapping))
        } else {
            let local = Affine::from_tag(tag, checkpoint)?;
            let local = if name.eq_ignore_ascii_case("svg") {
                let geometry = nested_viewport(tag, viewport, checkpoint)?;
                viewport = geometry.as_ref().and_then(|(_, size)| *size);
                local
                    .zip(geometry)
                    .and_then(|(transform, (mapping, _))| transform.then_local(mapping))
            } else {
                local
            };
            parent
                .and_then(|frame| frame.cumulative)
                .zip(local)
                .and_then(|(ancestor, local)| ancestor.then_local(local))
        };
        Ok(Self {
            cumulative,
            viewport,
            class_tokens: if retain_classes {
                parse_class_tokens(tag, checkpoint)?
            } else {
                Vec::new()
            },
        })
    }

    pub(super) fn foreign_object_transform<E>(
        &self,
        tag: &str,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Option<Affine>, E> {
        Ok(self
            .cumulative
            .zip(Affine::from_tag(tag, checkpoint)?)
            .and_then(|(ancestor, local)| ancestor.then_local(local)))
    }
}

pub(super) fn attribute<'a, E>(
    tag: &'a str,
    name: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<Cow<'a, str>>, E> {
    let mut cursor = 0;
    while let Some(attr) = next_svg_quoted_attr_with_checkpoints(tag, cursor, checkpoint)? {
        if &tag[attr.name_start..attr.name_end] == name {
            let raw = tag[attr.value_start..attr.value_end].trim();
            checkpoint()?;
            let decoded = quick_xml::escape::unescape(raw).unwrap_or(Cow::Borrowed(raw));
            checkpoint()?;
            return Ok(Some(decoded));
        }
        cursor = attr.full_end;
    }
    Ok(None)
}

fn parse_class_tokens<E>(
    tag: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<String>, E> {
    let Some(value) = attribute(tag, "class", checkpoint)? else {
        return Ok(Vec::new());
    };
    checkpoint()?;
    Ok(value.split_whitespace().map(str::to_owned).collect())
}

fn push_unique_token<E>(
    tokens: &mut Vec<String>,
    token: &str,
    iteration: &mut usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    for existing in tokens.iter() {
        checkpoint_loop(*iteration, checkpoint)?;
        *iteration = iteration.saturating_add(1);
        if existing == token {
            return Ok(());
        }
    }
    tokens.push(token.to_string());
    Ok(())
}

pub(super) fn source_class_attr_tokens<E>(
    stack: &[SourceFrame],
    foreign_object_tag: &str,
    inner: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<String>, E> {
    let mut tokens = Vec::new();
    let mut iteration = 0usize;
    for (frame_index, frame) in stack.iter().enumerate() {
        checkpoint_loop(frame_index, checkpoint)?;
        for token in &frame.class_tokens {
            push_unique_token(&mut tokens, token, &mut iteration, checkpoint)?;
        }
    }
    for tag in [foreign_object_tag, inner] {
        for token in parse_class_tokens(tag, checkpoint)? {
            push_unique_token(&mut tokens, &token, &mut iteration, checkpoint)?;
        }
    }
    checkpoint()?;
    if tokens.is_empty() {
        return Ok(None);
    }
    escape_xml_attr_with_checkpoints(&tokens.join(" "), checkpoint).map(Some)
}
