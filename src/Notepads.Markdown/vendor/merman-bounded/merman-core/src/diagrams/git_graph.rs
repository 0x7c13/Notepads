use crate::common_db::LangiumCommonDbFields;
use crate::diagram::{
    DiagramWarningFact, GIT_GRAPH_DUPLICATE_COMMIT_WARNING_RULE_ID, legacy_warning_messages,
};
use crate::diagrams::langium_common::{
    LangiumCommonFacts, parse_langium_common, parse_langium_string,
    push_langium_common_editor_fact, strip_langium_inline_comment,
};
use crate::sanitize::sanitize_text;
use crate::{
    EditorRenamePolicy, EditorSemanticFacts, EditorSemanticKind, EditorSemanticSymbol, Error,
    MermaidConfig, ParseMetadata, Result, SourceSpan, family,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub(crate) fn is_valid_editor_reference(candidate: &str) -> bool {
    is_gitgraph_reference(candidate)
}

const COMMIT_TYPE_NORMAL: i64 = 0;
const COMMIT_TYPE_REVERSE: i64 = 1;
const COMMIT_TYPE_HIGHLIGHT: i64 = 2;
const COMMIT_TYPE_MERGE: i64 = 3;
const COMMIT_TYPE_CHERRY_PICK: i64 = 4;

#[derive(Debug, Clone)]
struct Commit {
    id: String,
    message: String,
    seq: i64,
    commit_type: i64,
    tags: Vec<String>,
    parents: Vec<String>,
    branch: String,
    custom_type: Option<i64>,
    custom_id: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitGraphBranchRenderModel {
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitGraphCommitRenderModel {
    pub id: String,
    pub message: String,
    pub seq: i64,
    #[serde(rename = "type")]
    pub commit_type: i64,
    pub tags: Vec<String>,
    pub parents: Vec<String>,
    pub branch: String,
    #[serde(rename = "customType", skip_serializing_if = "Option::is_none")]
    pub custom_type: Option<i64>,
    #[serde(rename = "customId", skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitGraphRenderModel {
    #[serde(rename = "type")]
    pub diagram_type: String,
    pub commits: Vec<GitGraphCommitRenderModel>,
    pub branches: Vec<GitGraphBranchRenderModel>,
    #[serde(rename = "currentBranch")]
    pub current_branch: String,
    pub direction: String,
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(
        default,
        rename = "warningFacts",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub warning_facts: Vec<DiagramWarningFact>,
}

impl GitGraphRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct BranchConfig {
    order: i64,
}

#[derive(Debug, Clone)]
struct CommitDb {
    id: String,
    msg: String,
    commit_type: i64,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct BranchDb {
    name: String,
    order: i64,
}

#[derive(Debug, Clone)]
struct MergeDb {
    branch: String,
    id: Option<String>,
    commit_type: Option<i64>,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct CherryPickDb {
    id: String,
    target_id: String,
    parent: String,
    tags: Option<Vec<String>>,
}

#[derive(Debug)]
struct GitGraphDb {
    commits: HashMap<String, Commit>,
    commit_order: Vec<String>,
    branches: HashMap<String, Option<String>>,
    branch_config: HashMap<String, BranchConfig>,
    branch_config_order: Vec<String>,
    head: Option<String>,
    curr_branch: String,
    direction: String,
    seq: i64,
    warning_facts: Vec<DiagramWarningFact>,
    title: String,
    acc_title: String,
    acc_descr: String,
    prng: Option<XorShift64Star>,
}

#[derive(Debug, Clone)]
struct SpannedValue {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
enum GitGraphEditorFactRole {
    Entity,
    Reference,
    Payload,
}

#[derive(Debug, Clone)]
struct GitGraphEditorFact {
    value: SpannedValue,
    detail: &'static str,
    kind: EditorSemanticKind,
    role: GitGraphEditorFactRole,
}

#[derive(Debug, Clone)]
enum GitGraphOperation {
    Commit(CommitDb),
    Branch(BranchDb),
    Checkout(String),
    Merge(MergeDb),
    CherryPick(CherryPickDb),
}

#[derive(Debug, Clone)]
struct GitGraphCommand {
    operation: GitGraphOperation,
    editor_facts: Vec<GitGraphEditorFact>,
    statement_span: SourceSpan,
}

struct GitGraphCommandParseError {
    error: Box<Error>,
    editor_facts: Vec<GitGraphEditorFact>,
    recovery_span: SourceSpan,
}

enum GitGraphCommandParseAbort {
    Cancelled(crate::OperationCancelled),
    Invalid(GitGraphCommandParseError),
}

impl From<crate::OperationCancelled> for GitGraphCommandParseAbort {
    fn from(cancelled: crate::OperationCancelled) -> Self {
        Self::Cancelled(cancelled)
    }
}

impl From<GitGraphCommandParseError> for GitGraphCommandParseAbort {
    fn from(error: GitGraphCommandParseError) -> Self {
        Self::Invalid(error)
    }
}

struct GitGraphSemanticSource {
    model: GitGraphRenderModel,
    editor_facts: EditorSemanticFacts,
}

struct GitGraphParseFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
}

struct GitGraphHeader {
    direction: Option<String>,
    body_start: usize,
}

struct GitGraphSyntaxOutcome {
    commands: Vec<GitGraphCommand>,
    common: LangiumCommonFacts,
    editor_facts: EditorSemanticFacts,
    first_error: Option<Error>,
}

#[derive(Debug, Clone, Copy)]
struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    fn new(seed: u64) -> Self {
        let mut state = seed;
        if state == 0 {
            state = 1;
        }
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        // Mirrors the seeded upstream renderer script used by `xtask gen-upstream-svgs`:
        //   x ^= x >> 12; x ^= x << 25; x ^= x >> 27; return x * 0x2545F4914F6CDD1D (mod 2^64)
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    fn next_hex_digit(&mut self) -> u8 {
        // Seeded upstream uses `Math.floor(Math.random() * 16)` where `Math.random()` is derived
        // from `next_u64() >> 11` (53 bits). This is equivalent to taking the top nibble of
        // `next_u64()`.
        ((self.next_u64() >> 60) & 0xF) as u8
    }

    fn make_random_hex(&mut self, len: usize) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(len);
        for _ in 0..len {
            let idx = self.next_hex_digit() as usize;
            out.push(HEX[idx] as char);
        }
        out
    }
}

impl GitGraphDb {
    fn clear(&mut self, config: &MermaidConfig, prng_override: Option<XorShift64Star>) {
        self.commits.clear();
        self.commit_order.clear();
        self.branches.clear();
        self.branch_config.clear();
        self.branch_config_order.clear();
        self.head = None;
        self.direction = "LR".to_string();
        self.seq = 0;
        self.warning_facts.clear();
        self.title.clear();
        self.acc_title.clear();
        self.acc_descr.clear();

        // Mermaid gitGraph auto-generates commit ids using `utils.random({ length: 7 })`, which
        // depends on `Math.random()`. For deterministic test runs (and for reproducible upstream
        // SVG baselines), we allow injecting a seed.
        //
        // When unset, we keep Mermaid's non-deterministic behavior (random per run).
        self.prng = match prng_override {
            Some(prng) => Some(prng),
            None => {
                let mut prng = seeded_gitgraph_prng(config);
                if let Some(prng) = prng.as_mut() {
                    // The seeded upstream SVG renderer consumes one `Math.random()` value before
                    // the first gitGraph auto-id is minted.
                    let _ = prng.next_u64();
                }
                prng
            }
        };

        let main = config
            .get_str("gitGraph.mainBranchName")
            .unwrap_or("main")
            .to_string();
        let main_order = config_i64(config, "gitGraph.mainBranchOrder").unwrap_or(0);
        self.curr_branch = main.clone();

        self.branches.insert(main.clone(), None);
        self.branch_config
            .insert(main.clone(), BranchConfig { order: main_order });
        self.branch_config_order.push(main);
    }

    fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    fn next_id(&mut self) -> String {
        if let Some(prng) = self.prng.as_mut() {
            prng.make_random_hex(7)
        } else {
            crate::runtime::generated_id_hex("git-graph.commit-id", self.seq as u64, 7)
        }
    }

    fn commit(&mut self, mut commit_db: CommitDb, config: &MermaidConfig) {
        let id_raw = std::mem::take(&mut commit_db.id);
        let msg_raw = std::mem::take(&mut commit_db.msg);
        let tags_raw = std::mem::take(&mut commit_db.tags);

        let id = sanitize_text(&id_raw, config);
        let msg = sanitize_text(&msg_raw, config);
        let tags: Vec<String> = tags_raw
            .into_iter()
            .map(|t| sanitize_text(&t, config))
            .collect();

        let commit_id = if id.is_empty() {
            let seq = self.seq;
            format!("{seq}-{}", self.next_id())
        } else {
            id
        };

        let parents = self
            .head
            .as_ref()
            .map(|h| vec![h.clone()])
            .unwrap_or_default();

        let new_commit = Commit {
            id: commit_id.clone(),
            message: msg,
            seq: self.seq,
            commit_type: commit_db.commit_type,
            tags,
            parents,
            branch: self.curr_branch.clone(),
            custom_type: None,
            custom_id: None,
        };
        self.seq += 1;

        self.head = Some(new_commit.id.clone());
        if self.commits.contains_key(&new_commit.id) {
            self.warning_facts.push(DiagramWarningFact::new(
                GIT_GRAPH_DUPLICATE_COMMIT_WARNING_RULE_ID,
                format!("Commit ID {} already exists", new_commit.id),
            ));
        }

        let existed = self.commits.contains_key(&new_commit.id);
        self.commits.insert(new_commit.id.clone(), new_commit);
        if !existed {
            self.commit_order.push(commit_id.clone());
        }

        self.branches
            .insert(self.curr_branch.clone(), Some(commit_id));
    }

    fn branch(&mut self, mut branch_db: BranchDb, config: &MermaidConfig) -> Result<()> {
        branch_db.name = sanitize_text(&branch_db.name, config);
        if self.branches.contains_key(&branch_db.name) {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout {}\")",
                    branch_db.name
                ),
            ));
        }

        let head_id = self.head.clone();
        self.branches.insert(branch_db.name.clone(), head_id);
        self.branch_config.insert(
            branch_db.name.clone(),
            BranchConfig {
                order: branch_db.order,
            },
        );
        self.branch_config_order.push(branch_db.name.clone());
        self.checkout(&branch_db.name, config)?;
        Ok(())
    }

    fn checkout(&mut self, branch: &str, config: &MermaidConfig) -> Result<()> {
        let branch = sanitize_text(branch, config);
        if !self.branches.contains_key(&branch) {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Trying to checkout branch which is not yet created. (Help try using \"branch {}\")",
                    branch
                ),
            ));
        }
        self.curr_branch = branch.clone();
        let id = self.branches.get(&branch).cloned().unwrap_or_default();
        self.head = id;
        Ok(())
    }

    fn merge(&mut self, mut merge_db: MergeDb, config: &MermaidConfig) -> Result<()> {
        merge_db.branch = sanitize_text(&merge_db.branch, config);
        if let Some(custom_id) = merge_db.id.as_mut() {
            *custom_id = sanitize_text(custom_id, config);
            if custom_id.is_empty() {
                merge_db.id = None;
            }
        }

        let current_branch = self.curr_branch.clone();
        let other_branch = merge_db.branch.clone();

        if current_branch == other_branch {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "Incorrect usage of \"merge\". Cannot merge a branch to itself".to_string(),
            ));
        }

        let Some(current_head_id) = self
            .branches
            .get(&current_branch)
            .and_then(|id| id.as_ref())
        else {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Current branch ({})has no commits",
                    current_branch
                ),
            ));
        };
        let Some(current_commit) = self.commits.get(current_head_id).cloned() else {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Current branch ({})has no commits",
                    current_branch
                ),
            ));
        };

        if !self.branches.contains_key(&other_branch) {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Branch to be merged ({}) does not exist",
                    other_branch
                ),
            ));
        }

        let Some(other_head_id) = self.branches.get(&other_branch).and_then(|id| id.as_ref())
        else {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Branch to be merged ({}) has no commits",
                    other_branch
                ),
            ));
        };
        let Some(other_commit) = self.commits.get(other_head_id).cloned() else {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Branch to be merged ({}) has no commits",
                    other_branch
                ),
            ));
        };

        if current_commit.branch == other_branch {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!("Cannot merge branch '{}' into itself.", other_branch),
            ));
        }

        if current_commit.id == other_commit.id {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "Incorrect usage of \"merge\". Both branches have same head".to_string(),
            ));
        }

        if let Some(custom_id) = merge_db.id.as_ref()
            && self.commits.contains_key(custom_id)
        {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!(
                    "Incorrect usage of \"merge\". Commit with id:{} already exists, use different custom id",
                    custom_id
                ),
            ));
        }

        let verified_branch = other_head_id.clone();
        let merge_commit_id = match merge_db.id.clone() {
            Some(id) => id,
            None => {
                let seq = self.seq;
                format!("{seq}-{}", self.next_id())
            }
        };
        let custom_id_flag = merge_db.id.is_some();

        let tags: Vec<String> = merge_db
            .tags
            .into_iter()
            .map(|t| sanitize_text(&t, config))
            .collect();

        let new_commit = Commit {
            id: merge_commit_id.clone(),
            message: format!("merged branch {} into {}", other_branch, current_branch),
            seq: self.seq,
            commit_type: COMMIT_TYPE_MERGE,
            tags,
            parents: vec![current_commit.id, verified_branch],
            branch: current_branch.clone(),
            custom_type: merge_db.commit_type,
            custom_id: Some(custom_id_flag),
        };
        self.seq += 1;

        self.head = Some(new_commit.id.clone());
        self.commits.insert(new_commit.id.clone(), new_commit);
        self.commit_order.push(merge_commit_id.clone());
        self.branches
            .insert(current_branch.clone(), Some(merge_commit_id));
        Ok(())
    }

    fn cherry_pick(&mut self, mut cp: CherryPickDb, config: &MermaidConfig) -> Result<()> {
        cp.id = sanitize_text(&cp.id, config);
        cp.target_id = sanitize_text(&cp.target_id, config);
        cp.parent = sanitize_text(&cp.parent, config);

        if cp.id.is_empty() {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "Incorrect usage of \"cherryPick\". Source commit id should exist and provided"
                    .to_string(),
            ));
        }

        let Some(source_commit) = self.commits.get(&cp.id).cloned() else {
            return Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "Incorrect usage of \"cherryPick\". Source commit id should exist and provided"
                    .to_string(),
            ));
        };
        if !cp.parent.is_empty() && !(source_commit.parents.iter().any(|p| p == &cp.parent)) {
            return Err(Error::diagram_parse_fallback("gitGraph".to_string(), "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit.".to_string()));
        }

        if source_commit.commit_type == COMMIT_TYPE_MERGE && cp.parent.is_empty() {
            return Err(Error::diagram_parse_fallback("gitGraph".to_string(), "Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified.".to_string()));
        }

        if cp.target_id.is_empty() || !self.commits.contains_key(&cp.target_id) {
            if source_commit.branch == self.curr_branch {
                return Err(Error::diagram_parse_fallback(
                    "gitGraph".to_string(),
                    "Incorrect usage of \"cherryPick\". Source commit is already on current branch"
                        .to_string(),
                ));
            }

            let current_commit_id = self
                .branches
                .get(&self.curr_branch)
                .cloned()
                .unwrap_or(None);
            if current_commit_id.is_none() {
                return Err(Error::diagram_parse_fallback(
                    "gitGraph".to_string(),
                    format!(
                        "Incorrect usage of \"cherry-pick\". Current branch ({})has no commits",
                        self.curr_branch
                    ),
                ));
            }

            let tags = match cp.tags {
                Some(t) => t
                    .into_iter()
                    .map(|v| sanitize_text(&v, config))
                    .filter(|tag| !tag.is_empty())
                    .collect::<Vec<_>>(),
                None => {
                    let mut tag = format!("cherry-pick:{}", source_commit.id);
                    if source_commit.commit_type == COMMIT_TYPE_MERGE {
                        tag.push_str(&format!("|parent:{}", cp.parent));
                    }
                    vec![tag]
                }
            };

            let seq = self.seq;
            let new_id = format!("{seq}-{}", self.next_id());
            let parents = self
                .head
                .as_ref()
                .map(|h| vec![h.clone(), source_commit.id.clone()])
                .unwrap_or_default();
            let commit = Commit {
                id: new_id.clone(),
                message: format!(
                    "cherry-picked {} into {}",
                    source_commit.message, self.curr_branch
                ),
                seq: self.seq,
                commit_type: COMMIT_TYPE_CHERRY_PICK,
                tags,
                parents,
                branch: self.curr_branch.clone(),
                custom_type: None,
                custom_id: None,
            };
            self.seq += 1;

            self.head = Some(commit.id.clone());
            self.commits.insert(commit.id.clone(), commit);
            self.commit_order.push(new_id.clone());
            self.branches.insert(self.curr_branch.clone(), Some(new_id));
        }

        Ok(())
    }

    fn commits_in_seq_order_controlled(
        &self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Vec<Commit>> {
        let mut out = Vec::with_capacity(self.commits.len());
        for commit in self.commits.values() {
            control.checkpoint()?;
            out.push(commit.clone());
        }
        control.checkpoint()?;
        out.sort_by_key(|c| c.seq);
        control.checkpoint()?;
        Ok(out)
    }

    fn branches_in_order_controlled(
        &self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Vec<GitGraphBranchRenderModel>> {
        let mut entries: Vec<(String, f64)> = Vec::new();
        for (i, name) in self.branch_config_order.iter().enumerate() {
            control.checkpoint()?;
            let cfg = self.branch_config.get(name);
            let order = cfg.map(|c| c.order);
            let order_f = match order {
                Some(v) => v as f64,
                None => format!("0.{i}").parse::<f64>().unwrap_or(0.0),
            };
            entries.push((name.clone(), order_f));
        }

        control.checkpoint()?;
        entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let mut branches = Vec::with_capacity(entries.len());
        for (name, _) in entries {
            control.checkpoint()?;
            branches.push(GitGraphBranchRenderModel { name });
        }
        Ok(branches)
    }
}

fn config_i64(config: &MermaidConfig, dotted_path: &str) -> Option<i64> {
    let mut cur = config.as_value();
    for seg in dotted_path.split('.') {
        cur = cur.as_object()?.get(seg)?;
    }
    cur.as_i64()
}

fn seeded_gitgraph_prng(config: &MermaidConfig) -> Option<XorShift64Star> {
    config_i64(config, "gitGraph.seed")
        .and_then(|v| u64::try_from(v).ok())
        .filter(|v| *v != 0)
        .map(XorShift64Star::new)
}

fn commit_to_render_model(c: Commit) -> GitGraphCommitRenderModel {
    GitGraphCommitRenderModel {
        id: c.id,
        message: c.message,
        seq: c.seq,
        commit_type: c.commit_type,
        tags: c.tags,
        parents: c.parents,
        branch: c.branch,
        custom_type: c.custom_type,
        custom_id: c.custom_id,
    }
}

fn parse_commit_type(raw: &str) -> Result<i64> {
    match raw.trim() {
        "NORMAL" => Ok(COMMIT_TYPE_NORMAL),
        "REVERSE" => Ok(COMMIT_TYPE_REVERSE),
        "HIGHLIGHT" => Ok(COMMIT_TYPE_HIGHLIGHT),
        other => Err(Error::diagram_parse_fallback(
            "gitGraph".to_string(),
            format!("Unknown commit type: {other}"),
        )),
    }
}

struct LineParser<'a> {
    input: &'a str,
    pos: usize,
    base_offset: usize,
}

impl<'a> LineParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            base_offset: 0,
        }
    }

    fn with_base(mut self, base_offset: usize) -> Self {
        self.base_offset = base_offset;
        self
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws(&mut self, control: &crate::OperationControl) -> crate::OperationControlResult<()> {
        control.checkpoint()?;
        let mut next_checkpoint = self.pos.saturating_add(4096);
        while self.peek_char().is_some_and(|c| c.is_whitespace()) {
            self.bump();
            if self.pos >= next_checkpoint {
                control.checkpoint()?;
                next_checkpoint = self.pos.saturating_add(4096);
            }
        }
        Ok(())
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn parse_word_until_ws_or_colon_spanned(
        &mut self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Option<SpannedValue>> {
        self.skip_ws(control)?;
        let start = self.pos;
        let mut next_checkpoint = self.pos.saturating_add(4096);
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() || c == ':' {
                break;
            }
            self.bump();
            if self.pos >= next_checkpoint {
                control.checkpoint()?;
                next_checkpoint = self.pos.saturating_add(4096);
            }
        }
        if self.pos == start {
            return Ok(None);
        }
        Ok(Some(SpannedValue {
            text: self.input[start..self.pos].to_string(),
            span: SourceSpan::new(self.base_offset + start, self.base_offset + self.pos),
        }))
    }

    fn consume_argument_name(
        &mut self,
        name: &str,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Option<(SourceSpan, SourceSpan)>> {
        self.skip_ws(control)?;
        let rest = self.remaining();
        let Some(after_name) = rest.strip_prefix(name) else {
            return Ok(None);
        };
        if !after_name.starts_with(':') {
            return Ok(None);
        }
        let keyword_start = self.base_offset + self.pos;
        self.pos += name.len();
        let keyword = SourceSpan::new(keyword_start, keyword_start + name.len());
        let colon = SourceSpan::new(keyword.end, keyword.end + 1);
        self.pos += 1;
        Ok(Some((keyword, colon)))
    }

    fn parse_quoted_spanned(
        &mut self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<SpannedValue>> {
        self.skip_ws(control)?;
        let Some(parsed) = parse_langium_string(self.remaining(), self.base_offset + self.pos)
        else {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "expected quoted string".to_string(),
            )));
        };
        control.checkpoint()?;
        self.pos += parsed.consumed;
        Ok(Ok(SpannedValue {
            text: parsed.value,
            span: parsed.value_span,
        }))
    }

    fn parse_name_token_spanned(
        &mut self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<SpannedValue>> {
        self.skip_ws(control)?;
        if matches!(self.peek_char(), Some('"' | '\'')) {
            return self.parse_quoted_spanned(control);
        }
        let start = self.pos;
        let mut next_checkpoint = self.pos.saturating_add(4096);
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                break;
            }
            self.bump();
            if self.pos >= next_checkpoint {
                control.checkpoint()?;
                next_checkpoint = self.pos.saturating_add(4096);
            }
        }
        if self.pos == start {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "expected name".to_string(),
            )));
        }
        let text = &self.input[start..self.pos];
        if !is_gitgraph_reference_controlled(text, control)? {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!("invalid gitGraph reference: {text}"),
            )));
        }
        Ok(Ok(SpannedValue {
            text: text.to_string(),
            span: SourceSpan::new(self.base_offset + start, self.base_offset + self.pos),
        }))
    }

    fn parse_bare_token_spanned(
        &mut self,
        expected: &str,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<SpannedValue>> {
        self.skip_ws(control)?;
        let start = self.pos;
        let mut next_checkpoint = self.pos.saturating_add(4096);
        while self.peek_char().is_some_and(|ch| !ch.is_whitespace()) {
            self.bump();
            if self.pos >= next_checkpoint {
                control.checkpoint()?;
                next_checkpoint = self.pos.saturating_add(4096);
            }
        }
        if self.pos == start {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                format!("expected {expected}"),
            )));
        }
        Ok(Ok(SpannedValue {
            text: self.input[start..self.pos].to_string(),
            span: SourceSpan::new(self.base_offset + start, self.base_offset + self.pos),
        }))
    }

    fn expect_eof(
        &mut self,
        command: &str,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<()>> {
        self.skip_ws(control)?;
        if self.is_eof() {
            return Ok(Ok(()));
        }
        Ok(Err(Error::diagram_parse_fallback(
            "gitGraph".to_string(),
            format!("unexpected {command} argument: {}", self.remaining()),
        )))
    }
}

fn is_gitgraph_reference(value: &str) -> bool {
    fn is_word(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    let bytes = value.as_bytes();
    bytes.first().is_some_and(|byte| is_word(*byte))
        && bytes
            .last()
            .is_some_and(|byte| is_word(*byte) || *byte == b'-')
        && bytes
            .iter()
            .all(|byte| is_word(*byte) || matches!(*byte, b'-' | b'.' | b'/'))
}

fn is_gitgraph_reference_controlled(
    value: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<bool> {
    fn is_word(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    let bytes = value.as_bytes();
    if !bytes.first().is_some_and(|byte| is_word(*byte))
        || !bytes
            .last()
            .is_some_and(|byte| is_word(*byte) || *byte == b'-')
    {
        return Ok(false);
    }
    for (index, byte) in bytes.iter().enumerate() {
        if index % 4096 == 0 {
            control.checkpoint()?;
        }
        if !is_word(*byte) && !matches!(*byte, b'-' | b'.' | b'/') {
            return Ok(false);
        }
    }
    Ok(true)
}

fn parse_gitgraph_int(value: &SpannedValue) -> Result<i64> {
    let bytes = value.text.as_bytes();
    let valid = bytes == b"0"
        || (bytes
            .first()
            .is_some_and(|byte| matches!(*byte, b'1'..=b'9'))
            && bytes[1..].iter().all(u8::is_ascii_digit));
    if !valid {
        return Err(Error::diagram_parse_fallback(
            "gitGraph".to_string(),
            format!("invalid branch order: {}", value.text),
        ));
    }
    value
        .text
        .parse::<i64>()
        .map_err(|error| Error::diagram_parse_fallback("gitGraph".to_string(), error.to_string()))
}

impl GitGraphCommand {
    fn apply(&self, db: &mut GitGraphDb, effective_config: &MermaidConfig) -> Result<()> {
        let result = match &self.operation {
            GitGraphOperation::Commit(commit) => {
                db.commit(commit.clone(), effective_config);
                Ok(())
            }
            GitGraphOperation::Branch(branch) => db.branch(branch.clone(), effective_config),
            GitGraphOperation::Checkout(name) => db.checkout(name, effective_config),
            GitGraphOperation::Merge(merge) => db.merge(merge.clone(), effective_config),
            GitGraphOperation::CherryPick(cherry_pick) => {
                db.cherry_pick(cherry_pick.clone(), effective_config)
            }
        };
        result.map_err(|error| error.with_exact_span_if_missing(self.statement_span))
    }

    fn push_editor_facts_controlled(
        &self,
        facts: &mut EditorSemanticFacts,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<()> {
        for fact in &self.editor_facts {
            control.checkpoint()?;
            fact.push_to(facts);
        }
        Ok(())
    }
}

impl GitGraphEditorFact {
    fn push_to(&self, facts: &mut EditorSemanticFacts) {
        match self.role {
            GitGraphEditorFactRole::Entity => {
                push_gitgraph_entity_fact(facts, self.value.clone(), self.detail, self.kind)
            }
            GitGraphEditorFactRole::Reference => {
                push_gitgraph_reference_fact(facts, self.value.clone(), self.detail, self.kind)
            }
            GitGraphEditorFactRole::Payload => {
                push_gitgraph_payload_fact(facts, self.value.clone(), self.detail, self.kind)
            }
        }
    }
}

fn gitgraph_editor_fact(
    value: SpannedValue,
    detail: &'static str,
    kind: EditorSemanticKind,
    role: GitGraphEditorFactRole,
) -> GitGraphEditorFact {
    GitGraphEditorFact {
        value,
        detail,
        kind,
        role,
    }
}

fn command_parse_result<T>(
    result: Result<T>,
    editor_facts: &[GitGraphEditorFact],
    recovery_span: SourceSpan,
) -> std::result::Result<T, GitGraphCommandParseError> {
    result.map_err(|error| GitGraphCommandParseError {
        error: Box::new(error),
        editor_facts: editor_facts.to_vec(),
        recovery_span,
    })
}

fn unexpected_gitgraph_argument(
    command: &str,
    parser: &LineParser<'_>,
    editor_facts: Vec<GitGraphEditorFact>,
    statement_span: SourceSpan,
) -> GitGraphCommandParseError {
    GitGraphCommandParseError {
        error: Box::new(Error::diagram_parse_exact(
            "gitGraph".to_string(),
            format!("unexpected {command} argument: {}", parser.remaining()),
            statement_span,
        )),
        editor_facts,
        recovery_span: statement_span,
    }
}

fn parse_git_graph_command(
    raw: &str,
    line_start: usize,
    control: &crate::OperationControl,
) -> std::result::Result<Option<GitGraphCommand>, GitGraphCommandParseAbort> {
    control.checkpoint()?;
    let line = raw.trim_end_matches('\r');
    let visible = strip_langium_inline_comment(line);
    let trimmed = visible.trim();
    if trimmed.is_empty() || trimmed.starts_with("%%") {
        return Ok(None);
    }

    let trimmed_start = visible.len().saturating_sub(visible.trim_start().len());
    let statement_span = SourceSpan::new(
        line_start + trimmed_start,
        line_start + trimmed_start + trimmed.len(),
    );
    let mut parser = LineParser::new(trimmed).with_base(line_start + trimmed_start);
    let Some(command) = parser.parse_word_until_ws_or_colon_spanned(control)? else {
        return Ok(None);
    };
    let mut editor_facts = Vec::new();

    let operation = match command.text.as_str() {
        "commit" => {
            parser.skip_ws(control)?;
            let mut commit = CommitDb {
                id: String::new(),
                msg: String::new(),
                commit_type: COMMIT_TYPE_NORMAL,
                tags: Vec::new(),
            };
            loop {
                control.checkpoint()?;
                parser.skip_ws(control)?;
                if parser.is_eof() {
                    break;
                }
                if matches!(parser.peek_char(), Some('"' | '\'')) {
                    let message = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    commit.msg = message.text.clone();
                    editor_facts.push(gitgraph_editor_fact(
                        message,
                        "gitGraph commit message",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    continue;
                }
                if parser.consume_argument_name("id", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    commit.id = value.text.clone();
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph commit id",
                        EditorSemanticKind::Object,
                        GitGraphEditorFactRole::Entity,
                    ));
                    continue;
                }
                if parser.consume_argument_name("msg", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    commit.msg = value.text.clone();
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph commit message",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    continue;
                }
                if parser.consume_argument_name("tag", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    commit.tags.push(value.text.clone());
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph commit tag",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    continue;
                }
                if parser.consume_argument_name("type", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_bare_token_spanned("commit type", control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    editor_facts.push(gitgraph_editor_fact(
                        value.clone(),
                        "gitGraph commit type",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    commit.commit_type = command_parse_result(
                        parse_commit_type(&value.text),
                        &editor_facts,
                        statement_span,
                    )?;
                    continue;
                }
                return Err(unexpected_gitgraph_argument(
                    "commit",
                    &parser,
                    editor_facts,
                    statement_span,
                )
                .into());
            }
            GitGraphOperation::Commit(commit)
        }
        "branch" => {
            let name = command_parse_result(
                parser.parse_name_token_spanned(control)?,
                &editor_facts,
                statement_span,
            )?;
            editor_facts.push(gitgraph_editor_fact(
                name.clone(),
                "gitGraph branch",
                EditorSemanticKind::Variable,
                GitGraphEditorFactRole::Entity,
            ));
            let mut order = 0i64;
            parser.skip_ws(control)?;
            if !parser.is_eof() {
                let Some(_) = parser.consume_argument_name("order", control)? else {
                    return Err(unexpected_gitgraph_argument(
                        "branch",
                        &parser,
                        editor_facts,
                        statement_span,
                    )
                    .into());
                };
                let value = command_parse_result(
                    parser.parse_bare_token_spanned("branch order", control)?,
                    &editor_facts,
                    statement_span,
                )?;
                editor_facts.push(gitgraph_editor_fact(
                    value.clone(),
                    "gitGraph branch order",
                    EditorSemanticKind::String,
                    GitGraphEditorFactRole::Payload,
                ));
                order = command_parse_result(
                    parse_gitgraph_int(&value),
                    &editor_facts,
                    statement_span,
                )?;
            }
            command_parse_result(
                parser.expect_eof("branch", control)?,
                &editor_facts,
                statement_span,
            )?;
            GitGraphOperation::Branch(BranchDb {
                name: name.text,
                order,
            })
        }
        "checkout" | "switch" => {
            let name = command_parse_result(
                parser.parse_name_token_spanned(control)?,
                &editor_facts,
                statement_span,
            )?;
            editor_facts.push(gitgraph_editor_fact(
                name.clone(),
                "gitGraph branch",
                EditorSemanticKind::Variable,
                GitGraphEditorFactRole::Reference,
            ));
            command_parse_result(
                parser.expect_eof(command.text.as_str(), control)?,
                &editor_facts,
                statement_span,
            )?;
            GitGraphOperation::Checkout(name.text)
        }
        "merge" => {
            let branch = command_parse_result(
                parser.parse_name_token_spanned(control)?,
                &editor_facts,
                statement_span,
            )?;
            editor_facts.push(gitgraph_editor_fact(
                branch.clone(),
                "gitGraph merge branch",
                EditorSemanticKind::Variable,
                GitGraphEditorFactRole::Reference,
            ));
            let mut merge = MergeDb {
                branch: branch.text,
                id: None,
                commit_type: None,
                tags: Vec::new(),
            };
            loop {
                control.checkpoint()?;
                parser.skip_ws(control)?;
                if parser.is_eof() {
                    break;
                }
                if parser.consume_argument_name("id", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    merge.id = Some(value.text.clone());
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph merge id",
                        EditorSemanticKind::Object,
                        GitGraphEditorFactRole::Entity,
                    ));
                    continue;
                }
                if parser.consume_argument_name("tag", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    merge.tags.push(value.text.clone());
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph merge tag",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    continue;
                }
                if parser.consume_argument_name("type", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_bare_token_spanned("merge type", control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    editor_facts.push(gitgraph_editor_fact(
                        value.clone(),
                        "gitGraph merge type",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    merge.commit_type = Some(command_parse_result(
                        parse_commit_type(&value.text),
                        &editor_facts,
                        statement_span,
                    )?);
                    continue;
                }
                return Err(unexpected_gitgraph_argument(
                    "merge",
                    &parser,
                    editor_facts,
                    statement_span,
                )
                .into());
            }
            GitGraphOperation::Merge(merge)
        }
        "cherry-pick" => {
            let mut cherry_pick = CherryPickDb {
                id: String::new(),
                target_id: String::new(),
                parent: String::new(),
                tags: None,
            };
            loop {
                control.checkpoint()?;
                parser.skip_ws(control)?;
                if parser.is_eof() {
                    break;
                }
                if parser.consume_argument_name("id", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    cherry_pick.id = value.text.clone();
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph cherry-pick id",
                        EditorSemanticKind::Object,
                        GitGraphEditorFactRole::Reference,
                    ));
                    continue;
                }
                if parser.consume_argument_name("parent", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    cherry_pick.parent = value.text.clone();
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph cherry-pick parent",
                        EditorSemanticKind::Object,
                        GitGraphEditorFactRole::Reference,
                    ));
                    continue;
                }
                if parser.consume_argument_name("tag", control)?.is_some() {
                    let value = command_parse_result(
                        parser.parse_quoted_spanned(control)?,
                        &editor_facts,
                        statement_span,
                    )?;
                    cherry_pick
                        .tags
                        .get_or_insert_with(Vec::new)
                        .push(value.text.clone());
                    editor_facts.push(gitgraph_editor_fact(
                        value,
                        "gitGraph cherry-pick tag",
                        EditorSemanticKind::String,
                        GitGraphEditorFactRole::Payload,
                    ));
                    continue;
                }
                return Err(unexpected_gitgraph_argument(
                    "cherry-pick",
                    &parser,
                    editor_facts,
                    statement_span,
                )
                .into());
            }
            GitGraphOperation::CherryPick(cherry_pick)
        }
        _ => {
            return Err(GitGraphCommandParseError {
                error: Box::new(Error::diagram_parse_exact(
                    "gitGraph".to_string(),
                    format!("Unknown statement: {}", command.text),
                    command.span,
                )),
                editor_facts,
                recovery_span: statement_span,
            }
            .into());
        }
    };

    control.checkpoint()?;
    Ok(Some(GitGraphCommand {
        operation,
        editor_facts,
        statement_span,
    }))
}

pub(crate) fn parse_git_graph(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_git_graph_with_warning_facts(code, meta).map(family::WarningSemanticParse::into_model)
}

pub(crate) fn parse_git_graph_with_warning_facts(
    code: &str,
    meta: &ParseMetadata,
) -> Result<family::WarningSemanticParse> {
    let model = parse_git_graph_semantic_source(code, meta)?.model;
    let compatibility = render_model_to_compat_json(&model, meta)?;
    Ok(family::WarningSemanticParse::new(
        compatibility,
        model.warning_facts,
    ))
}

pub(crate) fn parse_git_graph_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<family::CombinedSemanticParse> {
    control.checkpoint()?;
    let parsed = family::CombinedSemanticParse::from_construction_with_warning_facts(
        construct_git_graph_semantic_source_controlled(code, meta, control)?,
        |source| {
            let compatibility = render_model_to_compat_json(&source.model, meta);
            (
                compatibility,
                source.editor_facts,
                source.model.warning_facts,
            )
        },
        |failure| (*failure.error, *failure.editor_facts),
    );
    control.checkpoint()?;
    Ok(parsed)
}

pub(crate) fn render_model_to_compat_json(
    model: &GitGraphRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let warnings = legacy_warning_messages(&model.warning_facts);
    let mut out = Map::with_capacity(11);
    out.insert(
        "type".to_string(),
        Value::String(model.diagram_type.clone()),
    );
    out.insert("commits".to_string(), json!(&model.commits));
    out.insert("branches".to_string(), json!(&model.branches));
    out.insert("currentBranch".to_string(), json!(&model.current_branch));
    out.insert("direction".to_string(), json!(&model.direction));
    if let Some(title) = &model.title {
        out.insert("title".to_string(), Value::String(title.clone()));
    }
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("warningFacts".to_string(), json!(&model.warning_facts));
    out.insert("warnings".to_string(), json!(warnings));
    out.insert(
        "config".to_string(),
        crate::config::clone_value_nonrecursive(meta.effective_config.as_value()),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
pub(crate) fn parse_git_graph_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<GitGraphRenderModel> {
    Ok(parse_git_graph_semantic_source(code, meta)?.model)
}

pub(crate) fn parse_git_graph_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<GitGraphRenderModel>> {
    let source = construct_git_graph_semantic_source_controlled(code, meta, control)?;
    control.checkpoint()?;
    Ok(source
        .map(|source| source.model)
        .map_err(|failure| *failure.error))
}

fn push_gitgraph_entity_fact(
    facts: &mut EditorSemanticFacts,
    value: SpannedValue,
    detail: &str,
    kind: EditorSemanticKind,
) {
    if value.text.is_empty() {
        return;
    }
    facts.push_symbol(
        EditorSemanticSymbol::new(
            value.text,
            Some(detail.to_string()),
            kind,
            value.span,
            value.span,
        )
        .with_rename_policy(EditorRenamePolicy::GitGraphReference),
    );
}

fn push_gitgraph_payload_fact(
    facts: &mut EditorSemanticFacts,
    value: SpannedValue,
    detail: &str,
    kind: EditorSemanticKind,
) {
    if value.text.is_empty() {
        return;
    }
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.text,
        Some(detail.to_string()),
        kind,
        value.span,
        value.span,
    ));
}

fn push_gitgraph_reference_fact(
    facts: &mut EditorSemanticFacts,
    value: SpannedValue,
    detail: &str,
    kind: EditorSemanticKind,
) {
    if value.text.is_empty() {
        return;
    }
    facts.push_symbol(
        EditorSemanticSymbol::reference(
            value.text,
            Some(detail.to_string()),
            kind,
            value.span,
            value.span,
        )
        .with_rename_policy(EditorRenamePolicy::GitGraphReference),
    );
}

fn parse_git_graph_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<GitGraphSemanticSource> {
    construct_git_graph_semantic_source(code, meta).map_err(|failure| *failure.error)
}

fn construct_git_graph_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<GitGraphSemanticSource, GitGraphParseFailure> {
    construct_git_graph_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_git_graph_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<GitGraphSemanticSource, GitGraphParseFailure>>
{
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("gitGraph");

    let header = match parse_gitgraph_header(code, control)? {
        Ok(header) => header,
        Err(error) => {
            return Ok(Err(gitgraph_parse_failure(
                error,
                EditorSemanticFacts::new(),
                SourceSpan::new(0, code.len()),
            )));
        }
    };
    let GitGraphSyntaxOutcome {
        commands,
        common,
        editor_facts,
        first_error,
    } = collect_gitgraph_commands(code, header.body_start, meta, control)?;
    if let Some(error) = first_error {
        return Ok(Err(gitgraph_parse_failure(
            error,
            editor_facts,
            SourceSpan::new(0, code.len()),
        )));
    }
    let common = LangiumCommonDbFields::from_facts(&common);
    let direction = header.direction;

    let effective_config = &meta.effective_config;
    let prng_override = if seeded_gitgraph_prng(effective_config).is_some() {
        // Upstream committed SVG fixtures are generated after a successful `mermaid.parse(code)`
        // followed by `mermaid.render(...)`. Seeded gitGraph auto ids consume the global
        // `Math.random()` stream during that warm-up parse, so mirror that state before building
        // the render model used for SVG parity.
        let mut warmup = new_gitgraph_db();
        warmup.clear(effective_config, None);
        apply_gitgraph_common_fields(&mut warmup, &common);
        if let Some(d) = direction.as_deref() {
            warmup.set_direction(d);
        }
        if let Err(error) =
            apply_git_graph_commands_controlled(&commands, &mut warmup, effective_config, control)?
        {
            return Ok(Err(gitgraph_parse_failure(
                error,
                editor_facts,
                SourceSpan::new(0, code.len()),
            )));
        }
        warmup.prng
    } else {
        None
    };

    let mut db = new_gitgraph_db();
    db.clear(effective_config, prng_override);
    apply_gitgraph_common_fields(&mut db, &common);
    if let Some(d) = direction {
        db.set_direction(&d);
    }
    if let Err(error) =
        apply_git_graph_commands_controlled(&commands, &mut db, effective_config, control)?
    {
        return Ok(Err(gitgraph_parse_failure(
            error,
            editor_facts,
            SourceSpan::new(0, code.len()),
        )));
    }

    let ordered_commits = db.commits_in_seq_order_controlled(control)?;
    let mut commits = Vec::with_capacity(ordered_commits.len());
    for commit in ordered_commits {
        control.checkpoint()?;
        commits.push(commit_to_render_model(commit));
    }
    let branches = db.branches_in_order_controlled(control)?;

    Ok(Ok(GitGraphSemanticSource {
        model: GitGraphRenderModel {
            diagram_type: meta.diagram_type.clone(),
            commits,
            branches,
            current_branch: db.curr_branch,
            direction: db.direction,
            title: if db.title.is_empty() {
                None
            } else {
                Some(db.title)
            },
            acc_title: if db.acc_title.is_empty() {
                None
            } else {
                Some(db.acc_title)
            },
            acc_descr: if db.acc_descr.is_empty() {
                None
            } else {
                Some(db.acc_descr)
            },
            warning_facts: db.warning_facts.clone(),
        },
        editor_facts,
    }))
}

fn gitgraph_parse_failure(
    error: Error,
    mut editor_facts: EditorSemanticFacts,
    fallback_span: SourceSpan,
) -> GitGraphParseFailure {
    let (message, span) = match &error {
        Error::DiagramParse { diagnostic, .. } => (
            diagnostic.message().to_string(),
            diagnostic.span().or(Some(fallback_span)),
        ),
        other => (other.to_string(), Some(fallback_span)),
    };
    editor_facts.mark_recovered_from_parse_error(message, span);
    GitGraphParseFailure {
        error: Box::new(error),
        editor_facts: Box::new(editor_facts),
    }
}

fn parse_gitgraph_header(
    code: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<GitGraphHeader>> {
    let mut offset = 0usize;
    while offset < code.len() {
        control.checkpoint()?;
        let (line, next_offset) = physical_line(code, offset);
        let visible = line.split_once("%%").map_or(line, |(before, _)| before);
        let trimmed = visible.trim_start();
        if trimmed.trim().is_empty() {
            offset = next_offset;
            continue;
        }
        let Some(after_keyword) = trimmed.strip_prefix("gitGraph") else {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "expected gitGraph header".to_string(),
            )));
        };
        if after_keyword
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_whitespace() && ch != ':' && !after_keyword.starts_with("%%"))
        {
            return Ok(Err(Error::diagram_parse_fallback(
                "gitGraph".to_string(),
                "expected gitGraph header".to_string(),
            )));
        }

        let leading = visible.len() - trimmed.len();
        let keyword_start = offset + leading;
        let keyword_end = keyword_start + "gitGraph".len();
        let body_start = keyword_end;
        let mut rest = &code[body_start..];
        let whitespace = rest
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
        rest = &rest[whitespace..];

        if rest.starts_with(':') {
            let colon = body_start + whitespace;
            return Ok(Ok(GitGraphHeader {
                direction: None,
                body_start: colon + 1,
            }));
        }
        for direction in ["LR", "TB", "BT"] {
            let Some(after_direction) = rest.strip_prefix(direction) else {
                continue;
            };
            if after_direction
                .chars()
                .next()
                .is_some_and(|ch| !ch.is_whitespace() && ch != ':')
            {
                continue;
            }
            let direction_ws = after_direction
                .chars()
                .take_while(|ch| matches!(ch, ' ' | '\t'))
                .map(char::len_utf8)
                .sum::<usize>();
            if after_direction.as_bytes().get(direction_ws) == Some(&b':') {
                let direction_start = body_start + whitespace;
                let direction_end = direction_start + direction.len();
                let colon = direction_end + direction_ws;
                return Ok(Ok(GitGraphHeader {
                    direction: Some(direction.to_string()),
                    body_start: colon + 1,
                }));
            }
        }
        return Ok(Ok(GitGraphHeader {
            direction: None,
            body_start,
        }));
    }

    Ok(Err(Error::diagram_parse_fallback(
        "gitGraph".to_string(),
        "empty input".to_string(),
    )))
}

fn collect_gitgraph_commands(
    code: &str,
    mut offset: usize,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<GitGraphSyntaxOutcome> {
    let mut commands = Vec::new();
    let mut common = LangiumCommonFacts::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_error = None;
    while offset < code.len() {
        control.checkpoint()?;
        if let Some(parsed) = parse_langium_common(code, offset) {
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "gitGraph");
            if let Some(diagnostic) = parsed.diagnostic {
                first_error.get_or_insert_with(|| {
                    Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        diagnostic.message,
                        diagnostic.span.start,
                    )
                });
            }
            common.push(parsed.fact);
            offset += parsed.consumed;
            continue;
        }
        let line_start = offset;
        let (line, next_offset) = physical_line(code, offset);
        offset = next_offset;
        match parse_git_graph_command(line, line_start, control) {
            Ok(Some(command)) => {
                command.push_editor_facts_controlled(&mut editor_facts, control)?;
                commands.push(command);
            }
            Ok(None) => {}
            Err(GitGraphCommandParseAbort::Cancelled(cancelled)) => return Err(cancelled),
            Err(GitGraphCommandParseAbort::Invalid(error)) => {
                for fact in error.editor_facts {
                    control.checkpoint()?;
                    fact.push_to(&mut editor_facts);
                }
                first_error.get_or_insert_with(|| {
                    (*error.error).with_exact_span_if_missing(error.recovery_span)
                });
            }
        }
    }
    Ok(GitGraphSyntaxOutcome {
        commands,
        common,
        editor_facts,
        first_error,
    })
}

fn apply_gitgraph_common_fields(db: &mut GitGraphDb, common: &LangiumCommonDbFields) {
    db.acc_descr = common.acc_descr.clone().unwrap_or_default();
    db.acc_title = common.acc_title.clone().unwrap_or_default();
    db.title = common.title.clone().unwrap_or_default();
}

fn physical_line(source: &str, offset: usize) -> (&str, usize) {
    let rest = &source[offset..];
    if let Some(newline) = rest.find('\n') {
        let line = rest[..newline]
            .strip_suffix('\r')
            .unwrap_or(&rest[..newline]);
        (line, offset + newline + 1)
    } else {
        (rest, source.len())
    }
}

fn new_gitgraph_db() -> GitGraphDb {
    GitGraphDb {
        commits: HashMap::new(),
        commit_order: Vec::new(),
        branches: HashMap::new(),
        branch_config: HashMap::new(),
        branch_config_order: Vec::new(),
        head: None,
        curr_branch: "main".to_string(),
        direction: "LR".to_string(),
        seq: 0,
        warning_facts: Vec::new(),
        title: String::new(),
        acc_title: String::new(),
        acc_descr: String::new(),
        prng: None,
    }
}

fn apply_git_graph_commands_controlled(
    commands: &[GitGraphCommand],
    db: &mut GitGraphDb,
    effective_config: &MermaidConfig,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<()>> {
    for command in commands {
        control.checkpoint()?;
        if let Err(error) = command.apply(db, effective_config) {
            return Ok(Err(error));
        }
    }

    Ok(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Engine, ParseDiagnosticSpanKind, ParseOptions, RenderSemanticModel};
    use futures::executor::block_on;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn test_meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "gitGraph".to_string(),
            config: MermaidConfig::default(),
            effective_config: MermaidConfig::default(),
            title: None,
        }
    }

    fn parse_err(text: &str) -> String {
        let engine = Engine::new();
        match block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err() {
            Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
            other => other.to_string(),
        }
    }

    fn parse_with_seed(text: &str, seed: i64) -> crate::ManagedSemanticJson {
        let engine = Engine::new().with_site_config(MermaidConfig::from_value(
            json!({ "gitGraph": { "seed": seed } }),
        ));
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn commit_ids(model: &Value) -> Vec<String> {
        model["commits"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn git_graph_parser_can_cancel_inside_commit_arguments() {
        let mut text = String::from("gitGraph\ncommit id:\"root\"");
        for index in 0..512 {
            text.push_str(&format!(" tag:\"tag-{index}\""));
        }
        text.push('\n');
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(20);

        assert!(matches!(
            construct_git_graph_semantic_source_controlled(&text, &test_meta(), &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn should_handle_gitgraph_definition_and_defaults() {
        let model = parse("gitGraph:\n commit\n");
        assert_eq!(model["commits"].as_array().unwrap().len(), 1);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "main");
        assert_eq!(model["direction"].as_str().unwrap(), "LR");
        assert_eq!(model["branches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn parse_gitgraph_render_model_uses_typed_variant_without_changing_json_parse() {
        let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
            "gitGraph": { "seed": 1 }
        })));
        let input = r#"
gitGraph TB:
title <script>alert(1)</script><b>Git title</b>
accTitle: Git accTitle
accDescr: Git accDescription
commit id:"C0"
branch feature
checkout feature
commit id:"F1" tag:"v1"
checkout main
merge feature id:"M1"
"#;

        let parsed = engine
            .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, "gitGraph");
        match parsed.model() {
            RenderSemanticModel::GitGraph(model) => {
                assert_eq!(model.diagram_type, "gitGraph");
                assert_eq!(model.direction, "TB");
                assert_eq!(model.current_branch, "main");
                assert_eq!(model.title.as_deref(), Some("<b>Git title</b>"));
                assert_eq!(model.acc_title.as_deref(), Some("Git accTitle"));
                assert_eq!(model.acc_descr.as_deref(), Some("Git accDescription"));
                assert_eq!(model.branches.len(), 2);
                assert_eq!(model.branches[0].name, "main");
                assert_eq!(model.commits.len(), 3);
                assert_eq!(model.commits[1].id, "F1");
                assert_eq!(model.commits[1].tags, vec!["v1".to_string()]);
                assert_eq!(model.commits[2].commit_type, COMMIT_TYPE_MERGE);
            }
            other => panic!("gitGraph render parse should return typed model, got {other:?}"),
        }

        let parsed_json = engine
            .parse_diagram_sync(input, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(parsed_json.model["type"], json!("gitGraph"));
        assert_eq!(parsed_json.model["direction"], json!("TB"));
        assert_eq!(parsed_json.model["currentBranch"], json!("main"));
        assert_eq!(parsed_json.model["title"], json!("<b>Git title</b>"));
        assert_eq!(parsed_json.model["accTitle"], json!("Git accTitle"));
        assert_eq!(parsed_json.model["branches"][0]["name"], json!("main"));
        assert_eq!(parsed_json.model["commits"][1]["id"], json!("F1"));
        assert_eq!(parsed_json.model["commits"][1]["tags"], json!(["v1"]));
        assert!(parsed_json.model.get("config").is_some());
    }

    #[test]
    fn parse_gitgraph_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = concat!(
            "gitGraph TB\n",
            "accTitle: Git title\n",
            "accDescr: Git description\n",
            "branch feature order: 2\n",
            "commit id:\"C1\" msg:\"commit message\" tag:\"v1\" type: HIGHLIGHT\n",
            "checkout feature\n",
            "merge feature id:\"M1\" tag:\"merge tag\"\n",
            "cherry-pick id:\"C1\" parent:\"P1\" tag:\"pick tag\"\n",
        );
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("gitGraph", text)
            .unwrap()
            .unwrap();

        assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "feature"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C1"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "M1"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "P1"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "commit message")
        );
    }

    #[test]
    fn gitgraph_usage_occurrences_are_typed_references_without_polluting_entities() {
        let text = concat!(
            "gitGraph\n",
            "commit id:\"ROOT\"\n",
            "branch feature\n",
            "checkout feature\n",
            "commit id:\"F1\"\n",
            "switch main\n",
            "commit id:\"M0\"\n",
            "merge feature id:\"M1\"\n",
            "branch release\n",
            "cherry-pick id:\"M1\" parent:\"M0\"\n",
        );
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("gitGraph", text)
            .unwrap()
            .expect("gitGraph editor facts");

        let roles_for = |name: &str| {
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == name)
                .map(|symbol| symbol.role)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            roles_for("feature"),
            vec![
                crate::EditorSemanticRole::Entity,
                crate::EditorSemanticRole::Reference,
                crate::EditorSemanticRole::Reference,
            ]
        );
        assert_eq!(
            roles_for("main"),
            vec![crate::EditorSemanticRole::Reference]
        );
        assert_eq!(
            roles_for("M1"),
            vec![
                crate::EditorSemanticRole::Entity,
                crate::EditorSemanticRole::Reference,
            ]
        );
        assert_eq!(
            roles_for("M0"),
            vec![
                crate::EditorSemanticRole::Entity,
                crate::EditorSemanticRole::Reference,
            ]
        );
        assert_eq!(
            roles_for("release"),
            vec![crate::EditorSemanticRole::Entity]
        );

        let feature_symbols = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "feature")
            .collect::<Vec<_>>();
        assert!(feature_symbols[0].role.contributes_completion());
        assert!(feature_symbols[0].role.contributes_outline());
        for reference in &feature_symbols[1..] {
            assert!(!reference.role.contributes_completion());
            assert!(!reference.role.contributes_outline());
            assert!(reference.role.contributes_references());
            assert_eq!(reference.kind, EditorSemanticKind::Variable);
        }

        for name in ["M1", "M0"] {
            let symbols = facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == name)
                .collect::<Vec<_>>();
            assert_eq!(symbols[0].kind, EditorSemanticKind::Object);
            assert_eq!(symbols[1].kind, EditorSemanticKind::Object);
            assert!(symbols[1].role.contributes_references());
            assert!(!symbols[1].role.contributes_completion());
            assert!(!symbols[1].role.contributes_outline());
        }
    }

    #[test]
    fn gitgraph_parser_recovery_keeps_later_semantic_facts_without_rescanning() {
        let text = concat!(
            "gitGraph\r\n",
            "commit id:\"C1\"\r\n",
            "checkout main trailing\r\n",
            "commit id:\"C2\" msg:\"后来\"\r\n",
        );
        crate::diagrams::langium_common::reset_family_syntax_construction_count("gitGraph");
        let facts = crate::family::test_support::editor_facts(
            parse_git_graph_json_and_editor_facts,
            text,
            &test_meta(),
        );

        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count("gitGraph"),
            1
        );
        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C1"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C2"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "后来"));
    }

    #[test]
    fn gitgraph_unknown_command_reports_exact_command_span() {
        let engine = Engine::new();
        let text = "gitGraph\n  frobnicate branch\n";
        let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = err else {
            panic!("expected gitGraph parse error");
        };

        let command_start = text.find("frobnicate").unwrap();
        assert_eq!(diagnostic.message(), "Unknown statement: frobnicate");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(
                command_start,
                command_start + "frobnicate".len()
            ))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn seeded_auto_commit_ids_match_upstream_seeded_svg_pipeline() {
        let model = parse_with_seed("gitGraph:\ncommit\n", 1);
        let ids = commit_ids(&model);
        assert_eq!(ids, vec!["0-5b722bd".to_string()]);
    }

    #[test]
    fn seeded_auto_commit_ids_are_direction_invariant() {
        let base = commit_ids(&parse_with_seed("gitGraph:\ncommit\n", 1));
        let tb = commit_ids(&parse_with_seed("gitGraph TB:\ncommit\n", 1));
        let bt = commit_ids(&parse_with_seed("gitGraph BT:\ncommit\n", 1));
        assert_eq!(base, tb);
        assert_eq!(base, bt);
        assert_eq!(base, vec!["0-5b722bd".to_string()]);
    }

    #[test]
    fn auto_commit_ids_are_deterministic_for_default_engine() {
        let first = commit_ids(&parse("gitGraph:\ncommit\ncommit\n"));
        let second = commit_ids(&parse("gitGraph:\ncommit\ncommit\n"));

        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_ne!(first[0], first[1]);
        assert!(first[0].starts_with("0-"));
        assert!(first[1].starts_with("1-"));
    }

    #[test]
    fn should_handle_set_direction_tb_and_bt() {
        let model = parse("gitGraph TB:\ncommit\n");
        assert_eq!(model["direction"].as_str().unwrap(), "TB");
        let model = parse("gitGraph BT:\ncommit\n");
        assert_eq!(model["direction"].as_str().unwrap(), "BT");
    }

    #[test]
    fn should_checkout_and_switch_branch() {
        let model = parse("gitGraph:\nbranch new\ncheckout new\n");
        assert_eq!(model["commits"].as_array().unwrap().len(), 0);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "new");

        let model = parse("gitGraph:\nbranch new\nswitch new\n");
        assert_eq!(model["commits"].as_array().unwrap().len(), 0);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "new");
    }

    #[test]
    fn should_add_commits_to_checked_out_branch() {
        let model = parse("gitGraph:\nbranch new\ncheckout new\ncommit\ncommit\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "new");
        assert_eq!(commits[0]["branch"].as_str().unwrap(), "new");
        assert_eq!(commits[1]["branch"].as_str().unwrap(), "new");
        assert_eq!(commits[1]["parents"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn should_handle_commit_with_args_and_message_variants() {
        let model = parse("gitGraph:\ncommit \"a commit\"\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["message"].as_str().unwrap(), "a commit");

        let model = parse("gitGraph:\ncommit msg: \"test commit\"\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits[0]["message"].as_str().unwrap(), "test commit");

        let model = parse("gitGraph:\ncommit id:\"1111\"\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits[0]["id"].as_str().unwrap(), "1111");

        let model = parse("gitGraph:\ncommit tag:\"test\"\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(
            commits[0]["tags"].as_array().unwrap()[0].as_str().unwrap(),
            "test"
        );

        let model = parse("gitGraph:\ncommit tag:\"a\" tag:\"b\"\n");
        let commits = model["commits"].as_array().unwrap();
        let tags = commits[0]["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].as_str().unwrap(), "a");
        assert_eq!(tags[1].as_str().unwrap(), "b");

        let model = parse("gitGraph:\ncommit type: HIGHLIGHT\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits[0]["type"].as_i64().unwrap(), 2);

        let model = parse("gitGraph:\ncommit id:\"1111\" tag: \"test tag\"\n");
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits[0]["id"].as_str().unwrap(), "1111");
        assert_eq!(
            commits[0]["tags"].as_array().unwrap()[0].as_str().unwrap(),
            "test tag"
        );

        let model = parse(
            "gitGraph:\ncommit id:\"1111\" type:REVERSE tag: \"test tag\" msg:\"test msg\"\n",
        );
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits[0]["id"].as_str().unwrap(), "1111");
        assert_eq!(commits[0]["type"].as_i64().unwrap(), 1);
        assert_eq!(commits[0]["message"].as_str().unwrap(), "test msg");
        assert_eq!(
            commits[0]["tags"].as_array().unwrap()[0].as_str().unwrap(),
            "test tag"
        );
    }

    #[test]
    fn gitgraph_commit_body_preserves_mixed_order_fields() {
        let model =
            parse("gitGraph:\ncommit \"mixed message\" id:\"C1\" tag:\"v1\" type:REVERSE\n");
        let commit = &model["commits"][0];

        assert_eq!(commit["id"], "C1");
        assert_eq!(commit["message"], "mixed message");
        assert_eq!(commit["tags"], json!(["v1"]));
        assert_eq!(commit["type"], COMMIT_TYPE_REVERSE);
    }

    #[test]
    fn gitgraph_uses_langium_string_escapes_and_quote_aware_inline_comments() {
        let model = parse(concat!(
            "gitGraph\n",
            "commit id:'C1' msg:\"line\\n100%% complete\" tag:'v\\t1' %% outside comment\n",
        ));
        let commit = &model["commits"][0];

        assert_eq!(commit["id"], "C1");
        assert_eq!(commit["message"], "linen100%% complete");
        assert_eq!(commit["tags"], json!(["vt1"]));
    }

    #[test]
    fn gitgraph_rejects_tokens_outside_the_langium_command_grammar() {
        let invalid_inputs = [
            "gitGraph\ncommit id:C1\n",
            "gitGraph\ncommit type:\"NORMAL\"\n",
            "gitGraph\ncommit \"message\" trailing\n",
            "gitGraph\nbranch feature order:\"2\"\n",
            "gitGraph\nbranch feature unknown:\"value\"\n",
            "gitGraph\nbranch feature\ncheckout feature trailing\n",
            concat!(
                "gitGraph\n",
                "commit id:\"C0\"\n",
                "branch feature\n",
                "commit id:\"F1\"\n",
                "checkout main\n",
                "merge feature unknown:\"value\"\n",
            ),
            "gitGraph\ncherryPick id:\"C1\"\n",
        ];

        for input in invalid_inputs {
            let _ = parse_err(input);
        }
    }

    #[test]
    fn gitgraph_recovery_reports_parser_diagnostic_with_exact_crlf_span() {
        let text = concat!(
            "gitGraph\r\n",
            "commit id:\"C1\"\r\n",
            "  checkout main trailing %% hidden\r\n",
            "commit id:\"C2\"\r\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_git_graph_json_and_editor_facts,
            text,
            &ParseMetadata {
                diagram_type: "gitGraph".to_string(),
                config: MermaidConfig::default(),
                effective_config: MermaidConfig::default(),
                title: None,
            },
        );
        let invalid = "checkout main trailing";
        let start = text.find(invalid).unwrap();

        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C1"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C2"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(start, start + invalid.len()))
                && diagnostic.message.contains("unexpected checkout argument")
        }));
    }

    #[test]
    fn gitgraph_editor_recovery_reports_database_validation_errors() {
        let text = concat!(
            "gitGraph\r\n",
            "commit id:\"C1\"\r\n",
            "  checkout missing  \r\n",
        );
        let invalid = "checkout missing";
        let start = text.find(invalid).unwrap();
        let expected_span = SourceSpan::new(start, start + invalid.len());
        let engine = Engine::new();

        let error = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect_err("checkout of an unknown branch must fail strict parsing");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected gitGraph parse diagnostic");
        };
        assert_eq!(diagnostic.span(), Some(expected_span));

        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("gitGraph", text)
            .unwrap()
            .expect("gitGraph editor recovery facts");
        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "C1"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "missing"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(expected_span)
                && diagnostic.message.contains("not yet created")
        }));
    }

    #[test]
    fn cherry_pick_filters_tags_that_sanitize_to_empty() {
        let model = parse(concat!(
            "gitGraph\n",
            "commit id:\"ZERO\"\n",
            "branch feature\n",
            "commit id:\"A\"\n",
            "checkout main\n",
            "cherry-pick id:\"A\" tag:\"<script>alert(1)</script>\" tag:\"kept\"\n",
        ));
        let commits = model["commits"].as_array().unwrap();

        assert_eq!(commits.last().unwrap()["tags"], json!(["kept"]));
    }

    #[test]
    fn commit_errors_on_unknown_fields() {
        let err =
            parse_err("gitGraph\ncommit id:\"2\" msg:\"Malformed commit\" oops:\"ignored\"\n");
        assert_eq!(err, "unexpected commit argument: oops:\"ignored\"");
    }

    #[test]
    fn should_handle_three_straight_commits() {
        let model = parse("gitGraph:\ncommit\ncommit\ncommit\n");
        assert_eq!(model["commits"].as_array().unwrap().len(), 3);
        assert_eq!(model["branches"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn should_handle_new_branch_creation_and_names() {
        let model = parse("gitGraph:\ncommit\nbranch testBranch\n");
        assert_eq!(model["commits"].as_array().unwrap().len(), 1);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "testBranch");
        assert_eq!(model["branches"].as_array().unwrap().len(), 2);

        let model = parse("gitGraph:\ncommit\nbranch azAZ_-./test\n");
        assert_eq!(model["currentBranch"].as_str().unwrap(), "azAZ_-./test");
        assert_eq!(model["branches"].as_array().unwrap().len(), 2);

        let model = parse("gitGraph:\ncommit\nbranch 1.0.1\n");
        assert_eq!(model["currentBranch"].as_str().unwrap(), "1.0.1");
        assert_eq!(model["branches"].as_array().unwrap().len(), 2);

        let model = parse("gitGraph:\ncommit\nbranch release-\n");
        assert_eq!(model["currentBranch"].as_str().unwrap(), "release-");

        for invalid in ["release/", "release."] {
            let error = parse_err(&format!("gitGraph:\ncommit\nbranch {invalid}\n"));
            assert!(
                error.contains("invalid gitGraph reference"),
                "{invalid}: {error}"
            );
        }
    }

    #[test]
    fn should_allow_quoted_branch_names_and_merge() {
        let model = parse(
            "gitGraph:\ncommit\nbranch \"branch\"\ncheckout \"branch\"\ncommit\ncheckout main\nmerge \"branch\"\n",
        );
        assert_eq!(model["commits"].as_array().unwrap().len(), 3);
        assert_eq!(model["currentBranch"].as_str().unwrap(), "main");
        assert_eq!(model["branches"].as_array().unwrap().len(), 2);
        assert_eq!(
            model["branches"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|b| b["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["main", "branch"]
        );
    }

    #[test]
    fn should_handle_branch_order_sorting() {
        let model = parse(
            "gitGraph:\ncommit\nbranch test1 order: 3\nbranch test2 order: 2\nbranch test3 order: 1\n",
        );
        assert_eq!(
            model["branches"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|b| b["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["main", "test3", "test2", "test1"]
        );

        let model = parse("gitGraph:\ncommit\nbranch test1 order: 1\nbranch test2\nbranch test3\n");
        assert_eq!(
            model["branches"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|b| b["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["main", "test2", "test3", "test1"]
        );
    }

    #[test]
    fn should_handle_merge_with_two_parents() {
        let model = parse(
            "gitGraph:\ncommit\nbranch testBranch\ncheckout testBranch\ncommit\ncheckout main\ncommit\nmerge testBranch\n",
        );
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 4);
        let merge = &commits[3];
        assert_eq!(merge["branch"].as_str().unwrap(), "main");
        assert_eq!(merge["parents"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn should_support_cherry_picking_commits() {
        let model = parse(
            "gitGraph\ncommit id: \"ZERO\"\nbranch develop\ncommit id:\"A\"\ncheckout main\ncherry-pick id:\"A\"\n",
        );
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[2]["branch"].as_str().unwrap(), "main");
        assert_eq!(
            commits[2]["tags"].as_array().unwrap()[0].as_str().unwrap(),
            "cherry-pick:A"
        );

        let model = parse(
            "gitGraph\ncommit id: \"ZERO\"\nbranch develop\ncommit id:\"A\"\ncheckout main\ncherry-pick id:\"A\" tag:\"MyTag\"\n",
        );
        let commits = model["commits"].as_array().unwrap();
        assert_eq!(
            commits[2]["tags"].as_array().unwrap()[0].as_str().unwrap(),
            "MyTag"
        );

        let model = parse(
            "gitGraph\ncommit id: \"ZERO\"\nbranch develop\ncommit id:\"A\"\ncheckout main\ncherry-pick id:\"A\" tag:\"\"\n",
        );
        let commits = model["commits"].as_array().unwrap();
        assert!(commits[2]["tags"].as_array().unwrap().is_empty());
    }

    #[test]
    fn should_support_cherry_picking_merge_commits_and_validate_parent() {
        let err = parse_err(
            "gitGraph\ncommit id: \"ZERO\"\nbranch feature\nbranch release\ncheckout feature\ncommit id: \"A\"\ncommit id: \"B\"\ncheckout main\nmerge feature id: \"M\"\ncheckout release\ncommit id: \"C\"\ncherry-pick id:\"M\"\n",
        );
        assert!(err.contains("Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified."));

        let err = parse_err(
            "gitGraph\ncommit id: \"ZERO\"\nbranch feature\nbranch release\ncheckout feature\ncommit id: \"A\"\ncommit id: \"B\"\ncheckout main\nmerge feature id: \"M\"\ncheckout release\ncommit id: \"C\"\ncherry-pick id:\"M\" parent: \"A\"\n",
        );
        assert!(err.contains("Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit."));
    }

    #[test]
    fn should_throw_error_when_try_to_branch_existing_branch() {
        let err = parse_err("gitGraph\ncommit\nbranch testBranch\ncommit\nbranch main\n");
        assert!(err.contains("Trying to create an existing branch."));

        let err = parse_err("gitGraph\ncommit\nbranch testBranch\ncommit\nbranch testBranch\n");
        assert!(err.contains("Trying to create an existing branch."));
    }

    #[test]
    fn should_throw_error_when_try_to_checkout_unknown_branch() {
        let err = parse_err("gitGraph\ncommit\ncheckout testBranch\n");
        assert_eq!(
            err,
            "Trying to checkout branch which is not yet created. (Help try using \"branch testBranch\")"
        );
    }

    #[test]
    fn should_throw_error_when_trying_to_merge_without_commits_or_unknown_branch() {
        let err = parse_err("gitGraph\nmerge testBranch\n");
        assert_eq!(
            err,
            "Incorrect usage of \"merge\". Current branch (main)has no commits"
        );

        let err = parse_err("gitGraph\ncommit\nmerge testBranch\n");
        assert_eq!(
            err,
            "Incorrect usage of \"merge\". Branch to be merged (testBranch) does not exist"
        );

        let err = parse_err("gitGraph\nbranch test1\ncheckout main\ncommit\nmerge test1\n");
        assert_eq!(
            err,
            "Incorrect usage of \"merge\". Branch to be merged (test1) has no commits"
        );
    }

    #[test]
    fn should_throw_error_when_trying_to_merge_branch_to_itself() {
        let err = parse_err("gitGraph\ncommit\nbranch testBranch\nmerge testBranch\n");
        assert_eq!(
            err,
            "Incorrect usage of \"merge\". Cannot merge a branch to itself"
        );
    }

    #[test]
    fn should_throw_error_when_using_existing_id_as_merge_id() {
        let err = parse_err(
            "gitGraph\ncommit id: \"1-111\"\nbranch testBranch\ncommit id: \"2-222\"\ncheckout main\nmerge testBranch id: \"1-111\"\n",
        );
        assert!(err.contains("Incorrect usage of \"merge\". Commit with id:1-111 already exists, use different custom id"));
    }

    #[test]
    fn should_throw_error_when_trying_to_merge_branches_having_same_heads() {
        let err =
            parse_err("gitGraph\ncommit\nbranch testBranch\ncheckout main\nmerge testBranch\n");
        assert_eq!(
            err,
            "Incorrect usage of \"merge\". Both branches have same head"
        );
    }

    #[test]
    fn should_handle_accessibility_title_and_description() {
        let model = parse(
            "gitGraph:\naccTitle: This is a title\naccDescr: This is a description\ncommit\n",
        );
        assert_eq!(model["accTitle"].as_str().unwrap(), "This is a title");
        assert_eq!(model["accDescr"].as_str().unwrap(), "This is a description");

        let model = parse(
            "gitGraph:\naccTitle: This is a title\naccDescr {\n  This is a description\n  using multiple lines\n}\ncommit\n",
        );
        assert_eq!(model["accTitle"].as_str().unwrap(), "This is a title");
        assert_eq!(
            model["accDescr"].as_str().unwrap(),
            "This is a description\nusing multiple lines"
        );
    }

    #[test]
    fn should_work_with_unsafe_properties_as_ids_and_branch_names() {
        for prop in ["__proto__", "constructor"] {
            let model = parse(&format!(
                "gitGraph\ncommit id:\"{prop}\"\nbranch {prop}\ncheckout {prop}\ncommit\ncheckout main\nmerge {prop}\n"
            ));
            assert_eq!(model["commits"].as_array().unwrap().len(), 3);
            assert_eq!(commit_ids(&model)[0], prop);
            assert_eq!(model["currentBranch"].as_str().unwrap(), "main");
            assert_eq!(model["branches"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn should_log_warning_when_two_commits_have_same_id() {
        let text = "gitGraph\ncommit id:\"working on MDR\"\ncommit id:\"working on MDR\"\n";
        let engine = Engine::new();
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("gitGraph compatibility parse succeeds")
            .expect("gitGraph compatibility parse returns a diagram");
        let typed = parse_git_graph_model_for_render(text, &parsed.meta)
            .expect("gitGraph typed parse succeeds");
        let projection = render_model_to_compat_json(&typed, &parsed.meta)
            .expect("gitGraph compatibility projection succeeds");
        let model = parsed.model;

        assert_eq!(projection, model);
        assert_eq!(projection["type"], json!("gitGraph"));
        assert!(projection["config"].is_object());
        assert!(projection.get("title").is_none());
        let warnings = model["warningFacts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.get("message").and_then(|message| message.as_str()))
            .collect::<Vec<_>>();
        assert!(warnings.contains(&"Commit ID working on MDR already exists"));
        assert_eq!(
            model["warnings"],
            json!(["Commit ID working on MDR already exists"])
        );
    }
}
