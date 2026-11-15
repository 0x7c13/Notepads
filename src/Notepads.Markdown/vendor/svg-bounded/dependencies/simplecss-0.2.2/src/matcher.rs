// Copyright 2019 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::{
    control::{event, Control},
    selector::{Combinator, SimpleSelectorType, SubSelector},
    Element,
};
use alloc::vec::Vec;
#[cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{BudgetVec, ResourceKind};
pub(crate) trait View<'a> {
    fn len(&self) -> usize;
    fn kind(&self, index: usize) -> SimpleSelectorType<'a>;
    fn combinator(&self, index: usize) -> Combinator;
    fn subselectors(&self, index: usize) -> &[SubSelector<'a>];
}
enum Owner<'a, E> {
    Borrowed(&'a E),
    Owned(E),
    Alias(usize),
}
struct Frame<'a, E> {
    index: usize,
    element: Owner<'a, E>,
    advance: bool,
}
enum Tail<T> {
    Plain(Vec<T>),
    #[cfg(feature = "notepads-markdown-resources")]
    Paid(BudgetVec<T>),
}
struct Stack<T> {
    head: Option<T>,
    tail: Tail<T>,
}
impl<T> Stack<T> {
    fn new(control: Option<&Control>) -> Self {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(c) = control {
            return Self {
                head: None,
                tail: Tail::Paid(BudgetVec::new(c.resources().clone(), ResourceKind::Scratch)),
            };
        }
        let _ = control;
        Self {
            head: None,
            tail: Tail::Plain(Vec::new()),
        }
    }
    fn len(&self) -> usize {
        usize::from(self.head.is_some())
            + match &self.tail {
                Tail::Plain(v) => v.len(),
                #[cfg(feature = "notepads-markdown-resources")]
                Tail::Paid(v) => v.len(),
            }
    }
    fn get(&self, index: usize) -> &T {
        if index == 0 {
            self.head.as_ref().unwrap()
        } else {
            match &self.tail {
                Tail::Plain(v) => &v[index - 1],
                #[cfg(feature = "notepads-markdown-resources")]
                Tail::Paid(v) => &v[index - 1],
            }
        }
    }
    fn get_mut(&mut self, index: usize) -> &mut T {
        if index == 0 {
            self.head.as_mut().unwrap()
        } else {
            match &mut self.tail {
                Tail::Plain(v) => &mut v[index - 1],
                #[cfg(feature = "notepads-markdown-resources")]
                Tail::Paid(v) => &mut v[index - 1],
            }
        }
    }
    fn push(&mut self, value: T) -> Result<(), ()> {
        if self.head.is_none() {
            self.head = Some(value);
            return Ok(());
        }
        match &mut self.tail {
            Tail::Plain(v) => {
                v.push(value);
                Ok(())
            }
            #[cfg(feature = "notepads-markdown-resources")]
            Tail::Paid(v) => v.try_push(value).map_err(|_| ()),
        }
    }
    fn pop(&mut self) {
        let tail = match &mut self.tail {
            Tail::Plain(v) => v.pop(),
            #[cfg(feature = "notepads-markdown-resources")]
            Tail::Paid(v) => v.pop(),
        };
        if tail.is_none() {
            self.head.take();
        }
    }
}
fn element<'s, E>(stack: &'s Stack<Frame<'_, E>>, mut index: usize) -> &'s E {
    loop {
        match &stack.get(index).element {
            Owner::Borrowed(e) => return e,
            Owner::Owned(e) => return e,
            Owner::Alias(i) => {
                debug_assert!(*i < index);
                index = *i;
            }
        }
    }
}
pub(crate) fn matches<'a, E: Element, V: View<'a>>(
    selector: &V,
    input: &E,
    control: Option<&Control>,
) -> Result<bool, ()> {
    if selector.len() == 0 {
        return Ok(false);
    }
    let mut stack = Stack::new(control);
    stack.push(Frame {
        index: selector.len() - 1,
        element: Owner::Borrowed(input),
        advance: false,
    })?;
    while stack.len() != 0 {
        event(control)?;
        let slot = stack.len() - 1;
        let index = stack.get(slot).index;
        if stack.get(slot).advance {
            let parent = element(&stack, slot).parent_element();
            if let Some(c) = control {
                c.checked()?;
            }
            if let Some(parent) = parent {
                stack.get_mut(slot).element = Owner::Owned(parent);
                stack.push(Frame {
                    index,
                    element: Owner::Alias(slot),
                    advance: false,
                })?;
            } else {
                stack.pop();
            }
            continue;
        }
        let mut accepted = true;
        if let SimpleSelectorType::Type(name) = selector.kind(index) {
            accepted = element(&stack, slot).has_local_name(name);
            if let Some(c) = control {
                c.checked()?;
            }
        }
        if accepted {
            for sub in selector.subselectors(index) {
                event(control)?;
                accepted = match sub {
                    SubSelector::Attribute(name, op) => {
                        element(&stack, slot).attribute_matches(name, *op)
                    }
                    SubSelector::PseudoClass(class) => {
                        element(&stack, slot).pseudo_class_matches(*class)
                    }
                };
                if let Some(c) = control {
                    c.checked()?;
                }
                if !accepted {
                    break;
                }
            }
        }
        if !accepted {
            stack.pop();
            continue;
        }
        let combinator = selector.combinator(index);
        if combinator == Combinator::None {
            return Ok(true);
        }
        let next = match combinator {
            Combinator::AdjacentSibling => element(&stack, slot).prev_sibling_element(),
            _ => element(&stack, slot).parent_element(),
        };
        if let Some(c) = control {
            c.checked()?;
        }
        if let Some(next) = next {
            let frame = stack.get_mut(slot);
            frame.index = index - 1;
            frame.element = Owner::Owned(next);
            frame.advance = combinator == Combinator::Descendant;
            if combinator == Combinator::Descendant {
                stack.push(Frame {
                    index: index - 1,
                    element: Owner::Alias(slot),
                    advance: false,
                })?;
            }
        } else {
            stack.pop();
        }
    }
    Ok(false)
}
