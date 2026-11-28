// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! An immutable source allocation; span checks inspect only its endpoints.

use super::{ChargedString, TaskStep, TaskWindow};
use crate::error::CoreError;
use crate::identity::SourceSpan;
use crate::policy::SourceBudget;
use std::marker::PhantomData;

pub(crate) struct TaskSourceText<'scope> {
    text: &'scope str,
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

impl<'scope> TaskWindow<'scope, '_> {
    pub(crate) fn source_text(
        &mut self,
        text: &'scope ChargedString,
        budget: &'scope SourceBudget,
    ) -> Result<TaskStep<TaskSourceText<'scope>>, CoreError> {
        if !self.binding::<ChargedString>()? {
            return Ok(TaskStep::Full);
        }
        self.parts().ensure_budget(budget)?;
        self.parts().ensure_source(&text._lease)?;
        Ok(TaskStep::Ready(TaskSourceText {
            text: text.as_str(),
            scope: PhantomData,
        }))
    }
}

impl<'scope> TaskSourceText<'scope> {
    pub(crate) fn validate_span(
        &self,
        span: SourceSpan,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        if !window.action((4 * size_of::<SourceSpan>() + 2 * size_of::<Self>() + 2) as u64)? {
            return Ok(TaskStep::Full);
        }
        let start = usize::try_from(span.start())
            .map_err(|_| window.fail(CoreError::ArithmeticOverflow))?;
        let end =
            usize::try_from(span.end()).map_err(|_| window.fail(CoreError::ArithmeticOverflow))?;
        if end > self.text.len()
            || !self.text.is_char_boundary(start)
            || !self.text.is_char_boundary(end)
        {
            return Err(window.fail(CoreError::InvalidBoundary));
        }
        Ok(TaskStep::Ready(()))
    }
}
