/// The details of an inline "shortcode" emoji/gemoji.
///
/// ("gemoji" name context: <https://github.com/github/gemoji>)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeShortCode {
    /// The shortcode that was resolved, e.g. "rabbit".
    pub code: String,

    /// The emoji `code` resolved to, e.g. "🐰".
    pub emoji: String,
}

impl NodeShortCode {
    /// Checks whether the input is a valid short code.
    pub fn resolve(code: &str) -> Option<Self> {
        let emoji = emojis::get_by_shortcode(code)?;
        Some(NodeShortCode {
            code: code.to_string(),
            emoji: emoji.to_string(),
        })
    }
}
