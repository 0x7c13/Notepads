// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Test-only entry/exit observation of the actual production bidi calls.
use markdown_svg_budget::ResourcesRef;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Draft,
    Reorder,
}

#[derive(Clone, Copy)]
pub(crate) struct Hooks {
    pub kind: Kind,
    pub enter: fn(&ResourcesRef),
    pub exit: fn(bool, usize),
}
thread_local! {
    static HOOKS: Cell<Option<Hooks>> = const { Cell::new(None) };
    static SHAPE_RETIRED: Cell<Option<fn(&ResourcesRef)>> = const { Cell::new(None) };
}
pub(crate) fn install(hooks: Option<Hooks>) -> Option<Hooks> {
    HOOKS.with(|slot| slot.replace(hooks))
}
pub(crate) fn install_shape_retired(hook: Option<fn(&ResourcesRef)>) -> Option<fn(&ResourcesRef)> {
    SHAPE_RETIRED.with(|slot| slot.replace(hook))
}
pub(crate) fn observe_shape_retired(resources: &ResourcesRef) {
    if let Some(hook) = SHAPE_RETIRED.with(|slot| slot.get()) {
        hook(resources);
    }
}

pub(crate) struct Guard {
    hooks: Option<Hooks>,
    completed: bool,
    length: usize,
}
pub(crate) fn begin(kind: Kind, resources: &ResourcesRef) -> Guard {
    let hooks = HOOKS
        .with(|slot| slot.get())
        .filter(|hooks| hooks.kind == kind);
    if let Some(hooks) = hooks {
        (hooks.enter)(resources);
    }
    Guard {
        hooks,
        completed: false,
        length: 0,
    }
}
impl Guard {
    pub(crate) fn complete(mut self, length: usize) {
        self.completed = true;
        self.length = length;
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(hooks) = self.hooks {
            (hooks.exit)(self.completed, self.length);
        }
    }
}
