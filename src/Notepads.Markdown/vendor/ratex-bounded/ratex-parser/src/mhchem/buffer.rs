//! Parser buffer (mirrors KaTeX mhchem `buffer` object).

#[derive(Clone, Default, Debug)]
pub struct Buffer {
    pub parenthesis_level: i32,
    pub begins_with_bond: bool,
    pub sb: bool,
    pub a: Option<String>,
    pub b: Option<String>,
    pub p: Option<String>,
    pub o: Option<String>,
    pub q: Option<String>,
    pub d: Option<String>,
    pub d_type: Option<String>,
    pub r: Option<String>,
    pub rdt: Option<String>,
    pub rd: Option<String>,
    pub rqt: Option<String>,
    pub rq: Option<String>,
    pub text_: Option<String>,
    pub rm: Option<String>,
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            parenthesis_level: 0,
            begins_with_bond: false,
            ..Default::default()
        }
    }

    /// Clear all fields except `parenthesis_level` and `begins_with_bond`.
    /// Clears inner [`String`] content to preserve capacity for reuse.
    pub fn clear_soft(&mut self) {
        self.sb = false;
        clear_opt_string(&mut self.a);
        clear_opt_string(&mut self.b);
        clear_opt_string(&mut self.p);
        clear_opt_string(&mut self.o);
        clear_opt_string(&mut self.q);
        clear_opt_string(&mut self.d);
        clear_opt_string(&mut self.d_type);
        clear_opt_string(&mut self.r);
        clear_opt_string(&mut self.rdt);
        clear_opt_string(&mut self.rd);
        clear_opt_string(&mut self.rqt);
        clear_opt_string(&mut self.rq);
        clear_opt_string(&mut self.text_);
        clear_opt_string(&mut self.rm);
    }

    /// Returns `true` if the slot is `None` or contains an empty string.
    pub fn is_slot_empty(slot: &Option<String>) -> bool {
        slot.as_ref().is_none_or(|s| s.is_empty())
    }

    /// Moves `v` into the slot without duplicating its text.
    pub fn set_slot(slot: &mut Option<String>, v: String) {
        *slot = Some(v);
    }

    pub fn clear_all(&mut self) {
        *self = Self::new();
    }
}

/// Clear the inner [`String`] if present, preserving its allocation for reuse.
fn clear_opt_string(slot: &mut Option<String>) {
    if let Some(s) = slot {
        s.clear();
    }
}
