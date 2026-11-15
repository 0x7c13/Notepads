// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#[cfg(feature = "notepads-markdown-resources")]
use core::cell::Cell;
#[cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourcesRef};

#[derive(Debug)]
pub(crate) struct Control {
    #[cfg(feature = "notepads-markdown-resources")]
    resources: ResourcesRef,
    #[cfg(feature = "notepads-markdown-resources")]
    events: Cell<u16>,
}
impl Control {
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn new(resources: ResourcesRef) -> Self {
        Self {
            resources,
            events: Cell::new(0),
        }
    }
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn resources(&self) -> &ResourcesRef {
        &self.resources
    }
    pub(crate) fn failed(&self) -> bool {
        #[cfg(feature = "notepads-markdown-resources")]
        {
            return self.resources.failure().is_some();
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        false
    }
    pub(crate) fn checked(&self) -> Result<(), ()> {
        if self.failed() {
            Err(())
        } else {
            Ok(())
        }
    }
    pub(crate) fn event(&self) -> Result<(), ()> {
        self.checked()?;
        #[cfg(feature = "notepads-markdown-resources")]
        {
            let credit = self.events.get();
            if credit == 0 {
                self.resources.checkpoint(256).map_err(|_| ())?;
                self.events.set(255);
            } else {
                self.events.set(credit - 1);
            }
        }
        Ok(())
    }
    pub(crate) fn permit(&self) -> Result<(), ()> {
        self.checked()?;
        #[cfg(feature = "notepads-markdown-resources")]
        {
            self.resources.scan_allowance().map_err(|_| ())?;
        }
        Ok(())
    }
    pub(crate) fn bytes(&self, n: usize) -> Result<(), ()> {
        self.checked()?;
        #[cfg(feature = "notepads-markdown-resources")]
        {
            self.resources.consume_scan_work(n).map_err(|_| ())?;
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        let _ = n;
        Ok(())
    }
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn error(&self) -> Error {
        self.resources.failure().unwrap_or_else(|| {
            self.resources
                .record(Error::new(FailureKind::Invariant, LimitKind::None))
        })
    }
}
pub(crate) fn event(control: Option<&Control>) -> Result<(), ()> {
    control.map_or(Ok(()), Control::event)
}
