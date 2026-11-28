// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned storage around the same CSS grammar and matcher.
use crate::{
    builder::SheetSink,
    control::Control,
    selector::{self, Combinator, SelectorSink, SimpleSelectorType, SubSelector},
    stream::Stream,
    Declaration, Element,
};
use core::ops::Range;
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};
struct Component<'a> {
    kind: SimpleSelectorType<'a>,
    combinator: Combinator,
    subs: Range<usize>,
}
struct Rule {
    components: Range<usize>,
    declarations: Range<usize>,
    specificity: [u8; 3],
    ordinal: usize,
}
/// A CSS sheet retaining all allocation fees until its actual owners retire.
pub struct StyleSheet<'a> {
    control: Control,
    rules: BudgetVec<Rule>,
    components: BudgetVec<Component<'a>>,
    subs: BudgetVec<SubSelector<'a>>,
    declarations: BudgetVec<Declaration<'a>>,
    ordinal: usize,
}
impl<'a> StyleSheet<'a> {
    /// Creates an empty sheet within the existing request.
    pub fn new(resources: ResourcesRef) -> Self {
        Self {
            control: Control::new(resources.clone()),
            rules: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
            components: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
            subs: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
            declarations: BudgetVec::new(resources, ResourceKind::Scratch),
            ordinal: 0,
        }
    }
    /// Adds CSS using the original syntax/recovery, preserving typed resource failures.
    pub fn try_parse_more(&mut self, text: &'a str) -> Result<(), Error> {
        self.control.checked().map_err(|()| self.control.error())?;
        let result = {
            let mut sink = FlatSheet {
                control: &self.control,
                rules: &mut self.rules,
                components: &mut self.components,
                subs: &mut self.subs,
                declarations: &mut self.declarations,
                ordinal: &mut self.ordinal,
                declaration_start: 0,
            };
            crate::builder::parse_more(text, &mut sink, Some(&self.control))
        };
        result.map_err(|()| self.control.error())?;
        let mut write = 0;
        for read in 0..self.rules.len() {
            self.control.event().map_err(|()| self.control.error())?;
            if !self.rules[read].declarations.is_empty() {
                if read != write {
                    self.rules.swap(read, write);
                }
                write += 1;
            }
        }
        while self.rules.len() > write {
            self.control.event().map_err(|()| self.control.error())?;
            self.rules.pop();
        }
        sort(&mut self.rules, &self.control).map_err(|()| self.control.error())?;
        self.control.checked().map_err(|()| self.control.error())
    }
    /// Borrows the rules in the same stable specificity/source order as legacy.
    pub fn rules(&self) -> impl Iterator<Item = RuleRef<'_, 'a>> {
        self.rules.iter().map(|rule| RuleRef { sheet: self, rule })
    }
}
/// A borrowed rule; it does not expose owned paid storage or infallible cloning.
#[derive(Clone, Copy)]
pub struct RuleRef<'s, 'a> {
    sheet: &'s StyleSheet<'a>,
    rule: &'s Rule,
}
impl<'a> RuleRef<'_, 'a> {
    /// The original borrowed declarations, shared by comma-separated selectors.
    pub fn declarations(&self) -> &[Declaration<'a>] {
        &self.sheet.declarations[self.rule.declarations.clone()]
    }
    /// The original saturating selector specificity.
    pub fn specificity(&self) -> [u8; 3] {
        self.rule.specificity
    }
    /// Matches through the shared iterative matcher, with paid backtracking storage.
    pub fn matches<E: Element>(&self, element: &E) -> Result<bool, Error> {
        self.sheet
            .control
            .checked()
            .map_err(|()| self.sheet.control.error())?;
        let result = crate::matcher::matches(self, element, Some(&self.sheet.control));
        self.sheet
            .control
            .checked()
            .map_err(|()| self.sheet.control.error())?;
        result.map_err(|()| self.sheet.control.error())
    }
}
impl<'a> crate::matcher::View<'a> for RuleRef<'_, 'a> {
    fn len(&self) -> usize {
        self.rule.components.len()
    }
    fn kind(&self, index: usize) -> SimpleSelectorType<'a> {
        self.sheet.components[self.rule.components.start + index].kind
    }
    fn combinator(&self, index: usize) -> Combinator {
        self.sheet.components[self.rule.components.start + index].combinator
    }
    fn subselectors(&self, index: usize) -> &[SubSelector<'a>] {
        let c = &self.sheet.components[self.rule.components.start + index];
        &self.sheet.subs[c.subs.clone()]
    }
}
struct FlatSelector<'s, 'a> {
    components: &'s mut BudgetVec<Component<'a>>,
    subs: &'s mut BudgetVec<SubSelector<'a>>,
    start: usize,
    specificity: [u8; 3],
}
impl<'a> SelectorSink<'a> for FlatSelector<'_, 'a> {
    type Output = (Range<usize>, [u8; 3]);
    fn len(&self) -> usize {
        self.components.len() - self.start
    }
    fn component(
        &mut self,
        kind: SimpleSelectorType<'a>,
        combinator: Combinator,
    ) -> Result<(), ()> {
        if matches!(kind, SimpleSelectorType::Type(_)) {
            self.specificity[2] = self.specificity[2].saturating_add(1);
        }
        let pos = self.subs.len();
        self.components
            .try_push(Component {
                kind,
                combinator,
                subs: pos..pos,
            })
            .map_err(|_| ())
    }
    fn sub(&mut self, value: SubSelector<'a>, combinator: Combinator) -> Result<(), ()> {
        if combinator != Combinator::None || self.len() == 0 {
            self.component(SimpleSelectorType::Universal, combinator)?;
        }
        let key = if matches!(value, SubSelector::Attribute("id", _)) {
            0
        } else {
            1
        };
        self.specificity[key] = self.specificity[key].saturating_add(1);
        self.subs.try_push(value).map_err(|_| ())?;
        self.components.last_mut().unwrap().subs.end = self.subs.len();
        Ok(())
    }
    fn finish(self) -> Self::Output {
        (self.start..self.components.len(), self.specificity)
    }
}
struct FlatSheet<'s, 'a> {
    control: &'s Control,
    rules: &'s mut BudgetVec<Rule>,
    components: &'s mut BudgetVec<Component<'a>>,
    subs: &'s mut BudgetVec<SubSelector<'a>>,
    declarations: &'s mut BudgetVec<Declaration<'a>>,
    ordinal: &'s mut usize,
    declaration_start: usize,
}
impl<'a> SheetSink<'a> for FlatSheet<'_, 'a> {
    type Selector = (Range<usize>, [u8; 3]);
    fn rule_count(&self) -> usize {
        self.rules.len()
    }
    fn selector(
        &mut self,
        text: &'a str,
        control: Option<&Control>,
    ) -> Result<(Option<Self::Selector>, usize), ()> {
        let start = self.components.len();
        let sub_start = self.subs.len();
        let result = selector::parse_into(
            text,
            control,
            FlatSelector {
                components: self.components,
                subs: self.subs,
                start,
                specificity: [0; 3],
            },
        )?;
        if result.0.is_none() {
            while self.components.len() > start {
                self.control.event()?;
                self.components.pop();
            }
            while self.subs.len() > sub_start {
                self.control.event()?;
                self.subs.pop();
            }
        }
        Ok(result)
    }

    fn rule(&mut self, (components, specificity): Self::Selector) -> Result<(), ()> {
        let ordinal = *self.ordinal;
        *self.ordinal = ordinal.checked_add(1).ok_or_else(|| {
            self.control
                .resources()
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity));
        })?;
        self.rules
            .try_push(Rule {
                components,
                specificity,
                ordinal,
                declarations: 0..0,
            })
            .map_err(|_| ())
    }
    fn begin_declarations(&mut self) {
        self.declaration_start = self.declarations.len();
    }
    fn declaration(&mut self, declaration: Declaration<'a>) -> Result<(), ()> {
        self.control.event()?;
        self.declarations.try_push(declaration).map_err(|_| ())
    }
    fn finish_group(&mut self, start: usize) -> Result<(), ()> {
        let range = self.declaration_start..self.declarations.len();
        if self.rules.len() == start {
            while self.declarations.len() > range.start {
                self.control.event()?;
                self.declarations.pop();
            }
        } else if range.is_empty() {
            let component_start = self.rules[start].components.start;
            let sub_start = self.components[component_start].subs.start;
            while self.rules.len() > start {
                self.control.event()?;
                self.rules.pop();
            }
            while self.components.len() > component_start {
                self.control.event()?;
                self.components.pop();
            }
            while self.subs.len() > sub_start {
                self.control.event()?;
                self.subs.pop();
            }
        } else {
            for rule in &mut self.rules[start..] {
                self.control.event()?;
                rule.declarations = range.clone();
            }
        }
        Ok(())
    }
}
fn sort(values: &mut [Rule], control: &Control) -> Result<(), ()> {
    fn less(a: &Rule, b: &Rule, control: &Control) -> Result<bool, ()> {
        control.event()?;
        Ok((a.specificity, a.ordinal) < (b.specificity, b.ordinal))
    }
    fn sift(v: &mut [Rule], mut root: usize, end: usize, c: &Control) -> Result<(), ()> {
        loop {
            let Some(mut child) = root.checked_mul(2).and_then(|x| x.checked_add(1)) else {
                return Ok(());
            };
            if child >= end {
                return Ok(());
            }
            if child + 1 < end && less(&v[child], &v[child + 1], c)? {
                child += 1;
            }
            if !less(&v[root], &v[child], c)? {
                return Ok(());
            }
            v.swap(root, child);
            root = child;
        }
    }
    for root in (0..values.len() / 2).rev() {
        sift(values, root, values.len(), control)?;
    }
    for end in (1..values.len()).rev() {
        values.swap(0, end);
        sift(values, 0, end, control)?;
    }
    Ok(())
}
/// A resource-aware view of the original inline declaration tokenizer.
pub struct DeclarationTokenizer<'a> {
    text: &'a str,
    pos: usize,
    finished: bool,
    control: Control,
}
impl<'a> DeclarationTokenizer<'a> {
    /// Borrows CSS text and retains the existing request capability.
    pub fn new(text: &'a str, resources: ResourcesRef) -> Self {
        Self {
            text,
            pos: 0,
            finished: false,
            control: Control::new(resources),
        }
    }
}
impl<'a> Iterator for DeclarationTokenizer<'a> {
    type Item = Result<Declaration<'a>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        if self.control.event().is_err() {
            self.finished = true;
            return Some(Err(self.control.error()));
        }
        let mut s = Stream::controlled(self.text, self.pos, &self.control);
        let _ = s.skip_spaces_and_comments();
        let result = if s.at_end() {
            None
        } else {
            crate::consume_declaration(&mut s).ok()
        };
        self.pos = s.pos();
        if self.control.failed() {
            self.finished = true;
            return Some(Err(self.control.error()));
        }
        if result.is_none() {
            self.finished = true;
        }
        result.map(Ok)
    }
}
