pub(crate) enum Action {
    SetDirection(String),
    SetAccTitle(String),
    SetAccDescr(String),

    AddNamespace {
        id: String,
        label: Option<String>,
    },
    AddClassesToNamespace {
        namespace: String,
        class_ids: Vec<String>,
    },
    PopNamespace,

    AddClass {
        id: String,
    },
    SetClassLabel {
        id: String,
        label: String,
    },
    SetCssClass {
        ids: String,
        css_class: String,
    },
    SetCssStyle {
        id: String,
        raw: String,
    },
    DefineClass {
        id: String,
        raw: String,
    },
    SetLink {
        id: String,
        url: String,
        target: Option<String>,
    },
    SetTooltip {
        id: String,
        tooltip: String,
    },
    SetClickEvent {
        id: String,
        function: String,
        args: Option<String>,
    },
    AddMembers {
        id: String,
        members: Vec<String>,
    },
    AddMember {
        id: String,
        member: String,
    },
    AddAnnotation {
        id: String,
        annotation: String,
    },
    AddRelation {
        data: RelationData,
    },
    AddNote {
        class_id: Option<String>,
        text: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Relation {
    pub type1: i32,
    pub type2: i32,
    pub line_type: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct RelationData {
    pub id1: String,
    pub id2: String,
    pub relation: Relation,
    pub relation_title1: Option<String>,
    pub relation_title2: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ActionList {
    head: Option<usize>,
    tail: Option<usize>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct ClassIdList {
    head: Option<usize>,
    tail: Option<usize>,
}

struct ActionRecord {
    action: Option<Action>,
    next: Option<usize>,
}

struct ClassIdRecord {
    id: Option<String>,
    next: Option<usize>,
}

/// Grammar fragments carry endpoints, while payloads stay in their original records.
#[derive(Default)]
pub(super) struct ActionArena {
    records: Vec<ActionRecord>,
    class_ids: Vec<ClassIdRecord>,
    construction_steps: usize,
    #[cfg(test)]
    action_links: usize,
    #[cfg(test)]
    class_id_links: usize,
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
                        expected_syntax: None,
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

    pub(super) fn push_class_id(
        &mut self,
        id: String,
        control: &crate::OperationControl,
    ) -> GrammarResult<ClassIdList> {
        self.checkpoint(control)?;
        let index = self.class_ids.len();
        self.class_ids.push(ClassIdRecord {
            id: Some(id),
            next: None,
        });
        Ok(ClassIdList {
            head: Some(index),
            tail: Some(index),
        })
    }

    pub(super) fn concat_namespace_items(
        &mut self,
        first: (ActionList, ClassIdList),
        rest: (ActionList, ClassIdList),
        control: &crate::OperationControl,
    ) -> GrammarResult<(ActionList, ClassIdList)> {
        let actions = self.concat(first.0, rest.0, control)?;
        if let (Some(tail), Some(head)) = (first.1.tail, rest.1.head) {
            self.class_ids[tail].next = Some(head);
            #[cfg(test)]
            {
                self.class_id_links += 1;
            }
        }
        Ok((
            actions,
            ClassIdList {
                head: first.1.head.or(rest.1.head),
                tail: rest.1.tail.or(first.1.tail),
            },
        ))
    }

    pub(super) fn namespace(
        &mut self,
        namespace: String,
        label: Option<String>,
        body: (ActionList, ClassIdList),
        control: &crate::OperationControl,
    ) -> GrammarResult<ActionList> {
        let mut class_ids = Vec::new();
        let mut next = body.1.head;
        while let Some(index) = next {
            self.checkpoint(control)?;
            let record = &mut self.class_ids[index];
            next = record.next;
            class_ids.extend(record.id.take());
        }
        let start = self.push_actions(
            [Action::AddNamespace {
                id: namespace.clone(),
                label,
            }],
            control,
        )?;
        let end = self.push_actions(
            [
                Action::AddClassesToNamespace {
                    namespace,
                    class_ids,
                },
                Action::PopNamespace,
            ],
            control,
        )?;
        let list = self.concat(start, body.0, control)?;
        self.concat(list, end, control)
    }

    pub(super) fn into_actions(self, list: ActionList) -> Actions {
        Actions {
            arena: self,
            next: list.head,
        }
    }

    #[cfg(test)]
    pub(super) fn carrier_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.records.len(),
            self.action_links,
            self.class_ids.len(),
            self.class_id_links,
        )
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
