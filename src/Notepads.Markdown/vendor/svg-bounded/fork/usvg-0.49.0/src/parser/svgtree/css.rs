// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::storage::Work;
use markdown_svg_budget::ResourcesRef;
use roxmltree::Error;
use simplecss::{Declaration, Element};
pub(super) enum StyleSheet<'a> {
    Plain(simplecss::StyleSheet<'a>),
    Paid(simplecss::controlled::StyleSheet<'a>),
}
impl<'a> StyleSheet<'a> {
    pub(super) fn new(resources: Option<&ResourcesRef>) -> Self {
        match resources {
            Some(r) => Self::Paid(simplecss::controlled::StyleSheet::new(r.clone())),
            None => Self::Plain(simplecss::StyleSheet::new()),
        }
    }
    pub(super) fn try_parse_more(&mut self, text: &'a str) -> Result<(), Error> {
        match self {
            Self::Plain(sheet) => {
                sheet.parse_more(text);
                Ok(())
            }
            Self::Paid(sheet) => sheet.try_parse_more(text).map_err(Error::Resource),
        }
    }
    pub(super) fn apply<E: Element>(
        &self,
        node: &E,
        work: &Work,
        mut declaration: impl FnMut(&Declaration<'a>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        match self {
            Self::Plain(sheet) => {
                for rule in &sheet.rules {
                    work.event()?;
                    let matched = rule.selector.matches(node);
                    work.checked()?;
                    if matched {
                        for d in &rule.declarations {
                            declaration(d)?;
                        }
                    }
                }
            }
            Self::Paid(sheet) => {
                for rule in sheet.rules() {
                    work.event()?;
                    if rule.matches(node).map_err(Error::Resource)? {
                        for d in rule.declarations() {
                            declaration(d)?;
                        }
                    }
                }
            }
        }
        work.checked()
    }
}
pub(super) fn inline<'a>(
    text: &'a str,
    work: &Work,
    mut declaration: impl FnMut(&Declaration<'a>) -> Result<(), Error>,
) -> Result<(), Error> {
    match work.resources() {
        Some(r) => {
            for item in simplecss::controlled::DeclarationTokenizer::new(text, r.clone()) {
                declaration(&item.map_err(Error::Resource)?)?;
            }
        }
        None => {
            for item in simplecss::DeclarationTokenizer::from(text) {
                declaration(&item)?;
            }
        }
    }
    work.checked()
}

#[cfg(test)]
#[path = "css_tests.rs"]
mod tests;
