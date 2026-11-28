pub(super) enum Action {
    SetTitle(String),
    SetAccTitle(String),
    SetAccDescr(String),

    EnsureParticipant {
        id: String,
    },
    AddParticipant {
        id: String,
        description: Option<String>,
        draw: String,
        config: Option<String>,
    },

    CreateParticipant {
        id: String,
        description: Option<String>,
        draw: String,
        config: Option<String>,
    },
    DestroyParticipant {
        id: String,
    },

    ControlSignal {
        signal_type: i32,
        text: Option<String>,
    },

    BoxStart {
        header: String,
    },
    BoxEnd,

    AddLinks {
        actor: String,
        text: String,
    },
    AddLink {
        actor: String,
        text: String,
    },
    AddProperties {
        actor: String,
        text: String,
    },
    AddDetails {
        actor: String,
        text: String,
    },

    AddMessage {
        from: String,
        to: String,
        signal_type: i32,
        text: String,
        activate: bool,
        central_connection: i32,
    },
    ActiveStart {
        actor: String,
    },
    ActiveEnd {
        actor: String,
    },
    CentralConnection {
        actor: String,
    },
    CentralConnectionReverse {
        actor: String,
    },

    AddNote {
        actors: Vec<String>,
        placement: i32,
        text: String,
    },

    Autonumber {
        start: Option<f64>,
        step: Option<f64>,
        visible: bool,
    },
}

#[derive(Clone, Copy, Default)]
pub(super) struct ActionList {
    head: Option<usize>,
    tail: Option<usize>,
}

struct ActionRecord {
    action: Option<Action>,
    next: Option<usize>,
}

/// Each event stays in one record as grammar reductions splice list endpoints.
#[derive(Default)]
pub(super) struct ActionArena {
    records: Vec<ActionRecord>,
    construction_steps: usize,
    #[cfg(test)]
    action_links: usize,
}

type GrammarResult<T> =
    std::result::Result<T, lalrpop_util::ParseError<usize, super::Tok, super::LexError>>;

impl ActionArena {
    fn checkpoint(&mut self, control: &crate::OperationControl) -> GrammarResult<()> {
        if self.construction_steps.is_multiple_of(128) {
            control
                .checkpoint()
                .map_err(|error| lalrpop_util::ParseError::User {
                    error: super::LexError {
                        message: error.to_string(),
                        span: crate::SourceSpan::new(0, 0),
                    },
                })?;
        }
        self.construction_steps = self.construction_steps.saturating_add(1);
        Ok(())
    }

    pub(super) fn push_actions(
        &mut self,
        actions: impl IntoIterator<Item = Action>,
        control: &crate::OperationControl,
    ) -> GrammarResult<ActionList> {
        let mut list = ActionList::default();
        for action in actions {
            self.checkpoint(control)?;
            let index = self.records.len();
            self.records.push(ActionRecord {
                action: Some(action),
                next: None,
            });
            list = self.concat(
                list,
                ActionList {
                    head: Some(index),
                    tail: Some(index),
                },
                control,
            )?;
        }
        Ok(list)
    }

    pub(super) fn concat(
        &mut self,
        first: ActionList,
        rest: ActionList,
        control: &crate::OperationControl,
    ) -> GrammarResult<ActionList> {
        self.checkpoint(control)?;
        if let (Some(tail), Some(head)) = (first.tail, rest.head) {
            self.records[tail].next = Some(head);
            #[cfg(test)]
            {
                self.action_links += 1;
            }
        }
        Ok(ActionList {
            head: first.head.or(rest.head),
            tail: rest.tail.or(first.tail),
        })
    }

    pub(super) fn wrap(
        &mut self,
        start: Action,
        body: ActionList,
        end: Action,
        control: &crate::OperationControl,
    ) -> GrammarResult<ActionList> {
        let start = self.push_actions([start], control)?;
        let end = self.push_actions([end], control)?;
        let list = self.concat(start, body, control)?;
        self.concat(list, end, control)
    }

    pub(super) fn branch_block(
        &mut self,
        start: Action,
        body: ActionList,
        branches: Vec<(String, ActionList)>,
        branch_type: i32,
        end: Action,
        control: &crate::OperationControl,
    ) -> GrammarResult<ActionList> {
        let start = self.push_actions([start], control)?;
        let mut list = self.concat(start, body, control)?;
        for (label, body) in branches {
            let branch = self.push_actions(
                [Action::ControlSignal {
                    signal_type: branch_type,
                    text: Some(label),
                }],
                control,
            )?;
            list = self.concat(list, branch, control)?;
            list = self.concat(list, body, control)?;
        }
        let end = self.push_actions([end], control)?;
        self.concat(list, end, control)
    }

    pub(super) fn into_actions(self, list: ActionList) -> Actions {
        Actions {
            arena: self,
            next: list.head,
        }
    }

    #[cfg(test)]
    pub(super) fn carrier_counts(&self) -> (usize, usize) {
        (self.records.len(), self.action_links)
    }
}

pub(super) struct Actions {
    arena: ActionArena,
    next: Option<usize>,
}

impl Iterator for Actions {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        let record = &mut self.arena.records[self.next?];
        self.next = record.next;
        record.action.take()
    }
}
