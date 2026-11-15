use crate::SourceSpan;

#[derive(Debug, Clone)]
pub struct StateStatementNote {
    pub position: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct StateStatementClick {
    pub id: String,
    pub url: String,
    pub tooltip: String,
}

#[derive(Debug, Clone)]
pub struct StateStatementState {
    pub id: String,
    pub id_span: Option<SourceSpan>,
    pub ty: String,
    pub description: Option<String>,
    pub descriptions: Vec<String>,
    pub doc: Option<StateDocumentId>,
    pub note: Option<StateStatementNote>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
    pub start: Option<bool>,
}

impl StateStatementState {
    pub fn new(id: String) -> Self {
        Self {
            id,
            id_span: None,
            ty: "default".to_string(),
            description: None,
            descriptions: Vec::new(),
            doc: None,
            note: None,
            classes: Vec::new(),
            styles: Vec::new(),
            text_styles: Vec::new(),
            start: None,
        }
    }

    pub fn new_typed(id: String, ty: &str) -> Self {
        Self {
            ty: ty.to_string(),
            ..Self::new(id)
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateStatementRelation {
    pub state1: StateStatementState,
    pub state2: StateStatementState,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StateStatement {
    Noop,
    State(StateStatementState),
    Relation(Box<StateStatementRelation>),
    ClassDef { id: String, classes: String },
    ApplyClass { ids: String, class_name: String },
    Style { ids: String, styles: String },
    Direction(String),
    AccTitle(String),
    AccDescr(String),
    Click(StateStatementClick),
}

/// Index of a document in a model-owned flat state document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateDocumentId(pub(crate) usize);

/// State statements owned once, with child documents referenced by index.
#[derive(Debug, Clone, Default)]
pub struct StateDocument {
    pub(crate) documents: Vec<Vec<StateStatement>>,
    pub(crate) root: Option<StateDocumentId>,
}

impl StateDocument {
    /// Returns the root document identifier, if one was parsed.
    pub fn root(&self) -> Option<StateDocumentId> {
        self.root
    }

    /// Borrows a document's statements without projecting a nested JSON tree.
    pub fn get(&self, id: StateDocumentId) -> Option<&[StateStatement]> {
        self.documents.get(id.0).map(Vec::as_slice)
    }

    /// Number of flat documents retained by this model.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub(crate) fn push(&mut self, statements: Vec<StateStatement>) -> StateDocumentId {
        let id = StateDocumentId(self.documents.len());
        self.documents.push(statements);
        id
    }

    pub(crate) fn root_statements(&self) -> &[StateStatement] {
        self.root.and_then(|id| self.get(id)).unwrap_or_default()
    }
}
