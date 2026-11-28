// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Preserve ordered first-match searches while paying each actual comparison.
use markdown_svg_budget::{Error, ResourcesRef};

pub(crate) struct SearchWork<'a> {
    resources: &'a ResourcesRef,
    credit: u64,
}

impl<'a> SearchWork<'a> {
    pub(crate) fn new(resources: &'a ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        Ok(Self {
            resources,
            credit: 0,
        })
    }

    pub(crate) fn charge(&mut self) -> Result<(), Error> {
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        if self.credit == 0 {
            self.resources.checkpoint(256)?;
            self.credit = 256;
        }
        self.credit -= 1;
        #[cfg(test)]
        phase::comparison();
        Ok(())
    }

    pub(crate) fn find<T>(
        &mut self,
        values: impl IntoIterator<Item = T>,
        mut matches: impl FnMut(&T) -> bool,
    ) -> Result<Option<T>, Error> {
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        for value in values {
            self.charge()?;
            if matches(&value) {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    pub(crate) fn finish(self) -> Result<(), Error> {
        self.resources.checkpoint(0)
    }
}

#[cfg(test)]
pub(crate) mod phase {
    use markdown_svg_budget::ResourcesRef;
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Stage {
        RunStyle,
        SourceSpan,
        RelevantRuns,
        IncludedRuns,
        StyleFace,
        RetainedKey,
        PendingKey,
        OutlineLookup,
    }
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum Event {
        Enter(Stage),
        Exit(Stage, usize, bool),
    }
    type Hook = fn(Event, &ResourcesRef);
    thread_local! {
        static HOOK: Cell<Option<Hook>> = const { Cell::new(None) };
        static ACTIVE: Cell<Option<(Stage, usize)>> = const { Cell::new(None) };
    }
    pub(crate) fn install(hook: Option<Hook>) -> Option<Hook> {
        HOOK.with(|slot| slot.replace(hook))
    }
    fn observe(event: Event, resources: &ResourcesRef) {
        HOOK.with(|slot| {
            if let Some(hook) = slot.get() {
                hook(event, resources);
            }
        });
    }
    pub(super) fn comparison() {
        ACTIVE.with(|slot| {
            if let Some((stage, count)) = slot.get() {
                slot.set(Some((stage, count + 1)));
            }
        });
    }
    pub(crate) struct Scope<'a> {
        resources: &'a ResourcesRef,
        stage: Stage,
        active: bool,
    }
    impl<'a> Scope<'a> {
        pub(crate) fn new(stage: Stage, resources: &'a ResourcesRef) -> Self {
            ACTIVE.with(|slot| assert!(slot.replace(Some((stage, 0))).is_none()));
            let scope = Self {
                resources,
                stage,
                active: true,
            };
            observe(Event::Enter(stage), resources);
            scope
        }
        fn end(&mut self, completed: bool) {
            if self.active {
                self.active = false;
                let (stage, compared) = ACTIVE.with(|slot| slot.take().unwrap());
                assert_eq!(stage, self.stage);
                observe(Event::Exit(stage, compared, completed), self.resources);
            }
        }
        pub(crate) fn finish(mut self) {
            self.end(true);
        }
    }
    impl Drop for Scope<'_> {
        fn drop(&mut self) {
            self.end(false);
        }
    }
}
