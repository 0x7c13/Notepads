// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_memory::FundedVec;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Attribute<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node<'a> {
    pub tag: &'a str,
    pub parent: Option<usize>,
    pub first_attribute: usize,
    pub attributes: usize,
    pub text: &'a str,
}

pub(crate) struct Tree<'a> {
    pub nodes: FundedVec<Node<'a>>,
    pub attributes: FundedVec<Attribute<'a>>,
    scope: MemoryScope,
}

impl<'a> Tree<'a> {
    pub fn work(&self, units: usize) -> Result<(), CanonicalError> {
        self.scope.charge_work(units).map_err(Into::into)
    }
    pub fn attr(&self, node: usize, name: &str) -> Option<&'a str> {
        let node = self.nodes[node];
        self.attributes.as_slice()[node.first_attribute..node.first_attribute + node.attributes]
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value)
    }

    pub fn classes(&self, node: usize) -> &'a str {
        self.attr(node, "class").unwrap_or("")
    }
    pub fn has_class(&self, node: usize, class: &str) -> bool {
        self.classes(node)
            .split_ascii_whitespace()
            .any(|c| c == class)
    }

    pub fn find_id(&self, id: &str) -> Result<usize, CanonicalError> {
        self.work(self.nodes.len().saturating_add(self.attributes.len()))?;
        let mut result = None;
        for (index, _) in self.nodes.iter().enumerate() {
            if self.attr(index, "id") == Some(id) && result.replace(index).is_some() {
                return Err(CanonicalError::InvalidReference);
            }
        }
        result.ok_or(CanonicalError::InvalidReference)
    }

    pub fn descendant(&self, mut child: usize, parent: usize) -> Result<bool, CanonicalError> {
        while let Some(next) = self.nodes[child].parent {
            self.work(1)?;
            if next == parent {
                return Ok(true);
            }
            child = next;
        }
        Ok(false)
    }

    pub fn parse(
        input: &'a str,
        scope: &MemoryScope,
        control: &OperationControl,
        limits: CanonicalLimits,
    ) -> Result<Self, CanonicalError> {
        scope.charge_work(input.len())?;
        if !input.chars().all(xml_scalar) {
            return Err(CanonicalError::MalformedXml);
        }
        let mut tree = Self {
            nodes: FundedVec::new_in(scope.clone(), Family::Canonical),
            attributes: FundedVec::new_in(scope.clone(), Family::Canonical),
            scope: scope.clone(),
        };
        let mut stack = [0usize; 64];
        let mut depth = 0usize;
        let mut cursor = 0usize;
        let mut root_closed = false;
        while cursor < input.len() {
            checkpoint(scope, control)?;
            if input[cursor..].starts_with("<!--") {
                let end = input[cursor + 4..]
                    .find("-->")
                    .ok_or(CanonicalError::MalformedXml)?;
                cursor += 4 + end + 3;
                continue;
            }
            if !input[cursor..].starts_with('<') {
                let end = input[cursor..]
                    .find('<')
                    .map_or(input.len(), |n| cursor + n);
                let value = &input[cursor..end];
                if !value.trim().is_empty()
                    || (depth > 0 && matches!(tree.nodes[stack[depth - 1]].tag, "text" | "tspan"))
                {
                    if depth == 0 {
                        return Err(CanonicalError::MalformedXml);
                    }
                    let first_attribute = tree.attributes.len();
                    push_node(
                        &mut tree,
                        Node {
                            tag: "#text",
                            parent: Some(stack[depth - 1]),
                            first_attribute,
                            attributes: 0,
                            text: value,
                        },
                        limits,
                    )?;
                }
                cursor = end;
                continue;
            }
            if input[cursor..].starts_with("<?") || input[cursor..].starts_with("<!") {
                return Err(CanonicalError::MalformedXml);
            }
            cursor += 1;
            if input[cursor..].starts_with('/') {
                cursor += 1;
                let name = name(input, &mut cursor)?;
                whitespace(input, &mut cursor);
                if depth == 0
                    || input.as_bytes().get(cursor) != Some(&b'>')
                    || tree.nodes[stack[depth - 1]].tag != name
                {
                    return Err(CanonicalError::MalformedXml);
                }
                cursor += 1;
                depth -= 1;
                if depth == 0 {
                    root_closed = true;
                }
                continue;
            }
            if depth == 0 && (root_closed || !tree.nodes.is_empty()) {
                return Err(CanonicalError::MalformedXml);
            }
            let tag = name(input, &mut cursor)?;
            if tag.contains(':') {
                return Err(CanonicalError::UnsupportedElement);
            }
            let index = tree.nodes.len();
            let first_attribute = tree.attributes.len();
            let mut closed = false;
            loop {
                whitespace(input, &mut cursor);
                match input.as_bytes().get(cursor) {
                    Some(b'>') => {
                        cursor += 1;
                        break;
                    }
                    Some(b'/') if input.as_bytes().get(cursor + 1) == Some(&b'>') => {
                        cursor += 2;
                        closed = true;
                        break;
                    }
                    None => return Err(CanonicalError::MalformedXml),
                    _ => {}
                }
                let key = name(input, &mut cursor)?;
                if tree.attributes.as_slice()[first_attribute..]
                    .iter()
                    .any(|a| a.name == key)
                {
                    return Err(CanonicalError::MalformedXml);
                }
                whitespace(input, &mut cursor);
                if input.as_bytes().get(cursor) != Some(&b'=') {
                    return Err(CanonicalError::MalformedXml);
                }
                cursor += 1;
                whitespace(input, &mut cursor);
                let quote = *input
                    .as_bytes()
                    .get(cursor)
                    .ok_or(CanonicalError::MalformedXml)?;
                if quote != b'\'' && quote != b'"' {
                    return Err(CanonicalError::MalformedXml);
                }
                cursor += 1;
                let start = cursor;
                let end = input[cursor..]
                    .find(quote as char)
                    .ok_or(CanonicalError::MalformedXml)?
                    + cursor;
                let value = &input[start..end];
                if value.contains('<') {
                    return Err(CanonicalError::MalformedXml);
                }
                cursor = end + 1;
                if tree.attributes.len() >= limits.attributes {
                    return Err(CanonicalError::Limited);
                }
                tree.attributes.try_push(Attribute { name: key, value })?;
            }
            let attributes = tree.attributes.len() - first_attribute;
            push_node(
                &mut tree,
                Node {
                    tag,
                    parent: depth.checked_sub(1).map(|d| stack[d]),
                    first_attribute,
                    attributes,
                    text: "",
                },
                limits,
            )?;
            if !closed {
                if depth >= limits.depth {
                    return Err(CanonicalError::Limited);
                }
                stack[depth] = index;
                depth += 1;
            } else if depth == 0 {
                root_closed = true;
            }
        }
        if depth != 0 || tree.nodes.first().map(|n| n.tag) != Some("svg") {
            return Err(CanonicalError::MalformedXml);
        }
        Ok(tree)
    }
}

fn push_node<'a>(
    tree: &mut Tree<'a>,
    node: Node<'a>,
    limits: CanonicalLimits,
) -> Result<(), CanonicalError> {
    if tree.nodes.len() >= limits.elements {
        return Err(CanonicalError::Limited);
    }
    tree.nodes.try_push(node).map_err(Into::into)
}

fn whitespace(input: &str, cursor: &mut usize) {
    while input
        .as_bytes()
        .get(*cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        *cursor += 1;
    }
}

fn name<'a>(input: &'a str, cursor: &mut usize) -> Result<&'a str, CanonicalError> {
    let start = *cursor;
    while input
        .as_bytes()
        .get(*cursor)
        .is_some_and(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
    {
        *cursor += 1;
    }
    if *cursor == start {
        return Err(CanonicalError::MalformedXml);
    }
    Ok(&input[start..*cursor])
}

fn xml_scalar(value: char) -> bool {
    matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

pub(crate) fn decode(value: &str, scope: &MemoryScope) -> Result<FundedString, CanonicalError> {
    let mut output = string(scope, "")?;
    let mut rest = value;
    while let Some(pos) = rest.find('&') {
        output.try_push_str(&rest[..pos])?;
        let entity = &rest[pos + 1..];
        let end = entity.find(';').ok_or(CanonicalError::MalformedXml)?;
        let entity = &entity[..end];
        let scalar = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let number = if let Some(number) = entity.strip_prefix("#x") {
                    u32::from_str_radix(number, 16).ok()
                } else if let Some(number) = entity.strip_prefix('#') {
                    number.parse::<u32>().ok()
                } else {
                    None
                };
                number
                    .and_then(char::from_u32)
                    .filter(|c| xml_scalar(*c))
                    .ok_or(CanonicalError::MalformedXml)?
            }
        };
        output.try_push(scalar)?;
        rest = &rest[pos + 1 + end + 1..];
    }
    output.try_push_str(rest)?;
    Ok(output)
}
