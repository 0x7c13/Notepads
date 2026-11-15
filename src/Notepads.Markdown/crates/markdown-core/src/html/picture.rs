// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use std::ops::Range;

use super::fragment::safe_destination;

pub(super) const MAX_PICTURES: usize = 128;
const MAX_VARIANT_BYTES: usize = 2048;

pub(crate) fn is_inline_delimiter(raw: &[u8], closing: bool) -> bool {
    let prefix: &[u8] = if closing { b"</picture" } else { b"<picture" };
    raw.get(..prefix.len())
        .is_some_and(|name| name.eq_ignore_ascii_case(prefix))
        && raw[prefix.len()..]
            .strip_suffix(b">")
            .is_some_and(|tail| tail.iter().all(|byte| matches!(byte, b' ' | b'\t')))
}

pub(crate) fn marker(window: &[u8]) -> bool {
    window
        .windows(8)
        .any(|name| name.eq_ignore_ascii_case(b"<picture"))
}

#[derive(Debug)]
pub(super) struct Source {
    pub source: Range<usize>,
    pub destination: String,
}

#[derive(Debug)]
pub(super) struct Picture {
    pub image: Range<usize>,
    pub light: Option<Source>,
    pub dark: Option<Source>,
}

impl Picture {
    pub fn variants(&mut self) -> impl Iterator<Item = &mut Source> {
        self.light.iter_mut().chain(self.dark.iter_mut())
    }

    pub fn offset(&mut self, source: usize, output: usize) {
        self.image = output + self.image.start..output + self.image.end;
        for variant in self.variants() {
            variant.source = source + variant.source.start..source + variant.source.end;
        }
    }

    pub fn storage_bytes(&self) -> usize {
        size_of::<Self>()
            + self
                .light
                .as_ref()
                .map_or(0, |source| source.destination.capacity())
            + self
                .dark
                .as_ref()
                .map_or(0, |source| source.destination.capacity())
    }
}

#[derive(Default)]
pub(super) struct Pending {
    pub saw_image: bool,
    pub image: Option<Range<usize>>,
    light: Option<Source>,
    dark: Option<Source>,
}

impl Pending {
    pub fn candidate(&mut self, element: &lol_html::html_content::Element<'_, '_>) -> Option<()> {
        let media = element.get_attribute("media")?;
        let media = htmlize::unescape_attribute(media);
        let query = media.trim().strip_prefix('(')?.strip_suffix(')')?;
        let (name, value) = query.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("prefers-color-scheme") {
            return None;
        }
        let slot = match value.trim() {
            value if value.eq_ignore_ascii_case("light") => &mut self.light,
            value if value.eq_ignore_ascii_case("dark") => &mut self.dark,
            _ => return None,
        };
        if slot.is_some() {
            return Some(());
        }
        let attribute = element
            .attributes()
            .iter()
            .find(|attribute| attribute.name() == "srcset")?;
        let source = attribute.value_source_location()?.bytes();
        if source.len() > MAX_VARIANT_BYTES {
            return None;
        }
        let value = htmlize::unescape_attribute(attribute.value());
        let mut tokens = value.split_ascii_whitespace();
        let url = tokens.next()?;
        if tokens.next().is_some_and(|descriptor| descriptor != "1x")
            || tokens.next().is_some()
            || url.contains([',', '<', '>'])
        {
            return None;
        }
        let destination = safe_destination(url, false)?.to_owned();
        *slot = Some(Source {
            source,
            destination,
        });
        Some(())
    }

    pub fn finish(self) -> Option<Picture> {
        Some(Picture {
            image: self.image?,
            light: self.light,
            dark: self.dark,
        })
    }
}
