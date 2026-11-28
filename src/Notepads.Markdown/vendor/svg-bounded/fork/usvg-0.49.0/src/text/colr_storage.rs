// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! COLR's concrete XML/path owners. Both modes use the original serialization
//! core; request storage cannot expose an ordinary String or mutable byte Vec.
use crate::tree::owner::{self, Result, TreeString};
use crate::ProcessingContext;
use std::fmt::{Arguments, Display};
pub(super) struct ColrWriter<'p> {
    mode: WriterMode,
    pub(super) processing: &'p ProcessingContext,
}
enum WriterMode {
    Plain {
        writer: Option<xmlwriter::XmlWriter>,
        output: Option<String>,
    },
    Paid(xmlwriter::RequestWriter),
}
impl<'p> ColrWriter<'p> {
    pub(super) fn new(options: xmlwriter::Options, p: &'p ProcessingContext) -> Result<Self> {
        owner::check(p, 0)?;
        let mode = match p.resources() {
            None => WriterMode::Plain {
                writer: Some(xmlwriter::XmlWriter::new(options)),
                output: None,
            },
            Some(r) => WriterMode::Paid(
                xmlwriter::RequestWriter::try_new(options, r).map_err(|e| p.record_resource(e))?,
            ),
        };
        Ok(Self {
            mode,
            processing: p,
        })
    }
    pub(super) fn start_element(&mut self, name: &str) -> Result<()> {
        let p = self.processing;
        owner::check(p, 0)?;
        let result = match &mut self.mode {
            WriterMode::Plain { writer, .. } => {
                writer
                    .as_mut()
                    .ok_or_else(|| owner::reject(p, None))?
                    .start_element(name);
                Ok(())
            }
            WriterMode::Paid(v) => v.try_start_element(name),
        };
        result.map_err(|e| p.record_resource(e))
    }
    pub(super) fn write_attribute(
        &mut self,
        name: &str,
        value: &(impl Display + ?Sized),
    ) -> Result<()> {
        let p = self.processing;
        owner::check(p, 0)?;
        let result = match &mut self.mode {
            WriterMode::Plain { writer, .. } => {
                writer
                    .as_mut()
                    .ok_or_else(|| owner::reject(p, None))?
                    .write_attribute(name, value);
                Ok(())
            }
            WriterMode::Paid(v) => v.try_write_attribute(name, value),
        };
        result.map_err(|e| p.record_resource(e))
    }
    pub(super) fn write_attribute_fmt(&mut self, name: &str, args: Arguments<'_>) -> Result<()> {
        let p = self.processing;
        owner::check(p, 0)?;
        let result = match &mut self.mode {
            WriterMode::Plain { writer, .. } => {
                writer
                    .as_mut()
                    .ok_or_else(|| owner::reject(p, None))?
                    .write_attribute_fmt(name, args);
                Ok(())
            }
            WriterMode::Paid(v) => v.try_write_attribute_fmt(name, args),
        };
        result.map_err(|e| p.record_resource(e))
    }
    pub(super) fn end_element(&mut self) -> Result<()> {
        let p = self.processing;
        owner::check(p, 0)?;
        let result = match &mut self.mode {
            WriterMode::Plain { writer, .. } => {
                writer
                    .as_mut()
                    .ok_or_else(|| owner::reject(p, None))?
                    .end_element();
                Ok(())
            }
            WriterMode::Paid(v) => v.try_end_element(),
        };
        result.map_err(|e| p.record_resource(e))
    }
    pub(super) fn finish(&mut self) -> Result<&[u8]> {
        let p = self.processing;
        owner::check(p, 0)?;
        match &mut self.mode {
            WriterMode::Plain { writer, output } => {
                if let Some(writer) = writer.take() {
                    *output = Some(writer.end_document());
                }
                Ok(output.as_ref().expect("finished plain XML").as_bytes())
            }
            WriterMode::Paid(v) => v.try_finish().map_err(|e| p.record_resource(e)),
        }
    }
}
pub(super) struct ColrPath<'p> {
    value: TreeString,
    logical_len: usize,
    pub(super) processing: &'p ProcessingContext,
}
impl<'p> ColrPath<'p> {
    pub(super) fn new(p: &'p ProcessingContext) -> Result<Self> {
        Ok(Self {
            value: TreeString::try_with_capacity(256, p)?,
            logical_len: 0,
            processing: p,
        })
    }
    pub(super) fn clear(&mut self) -> Result<()> {
        self.value.try_clear(self.processing)?;
        self.logical_len = 0;
        Ok(())
    }
    pub(super) fn write_fmt(&mut self, args: Arguments<'_>) -> Result<()> {
        self.value.try_write_fmt(args, self.processing)?;
        self.logical_len = self.value.len();
        Ok(())
    }
    pub(super) fn push_str(&mut self, text: &str) -> Result<()> {
        self.value.try_push_str(text, self.processing)?;
        self.logical_len = self.value.len();
        Ok(())
    }
    pub(super) fn finish_outline(&mut self) -> Result<()> {
        owner::check(self.processing, 0)?;
        self.logical_len = self.value.len().saturating_sub(1);
        Ok(())
    }
    pub(super) fn as_str(&self) -> &str {
        &self.value.as_str()[..self.logical_len]
    }
}
impl Display for ColrPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
