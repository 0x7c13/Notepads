// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Controlled request-owned main DOM allocation; original license retained.
use crate::storage::{XmlText, XmlVec};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;
use memchr::{memchr, memchr2};

use crate::{
    AttributeData, Document, ExpandedNameIndexed, NamespaceIdx, Namespaces, NodeData, NodeId,
    NodeKind, ShortRange, StringStorage, TextPos, NS_XMLNS_URI, NS_XML_PREFIX, NS_XML_URI, PI,
    XMLNS,
};

use crate::tokenizer::{self, Reference, StrSpan, Stream};

type Result<T> = core::result::Result<T, Error>;

/// A list of all possible errors.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    /// A typed failure from the active controlled request.
    #[cfg(feature = "notepads-markdown-resources")]
    Resource(markdown_svg_budget::Error),
    /// The `xmlns:xml` attribute must have an <http://www.w3.org/XML/1998/namespace> URI.
    InvalidXmlPrefixUri(TextPos),

    /// Only the `xmlns:xml` attribute can have the <http://www.w3.org/XML/1998/namespace> URI.
    UnexpectedXmlUri(TextPos),

    /// The <http://www.w3.org/2000/xmlns/> URI must not be declared.
    UnexpectedXmlnsUri(TextPos),

    /// `xmlns` can't be used as an element prefix.
    InvalidElementNamePrefix(TextPos),

    /// A namespace was already defined on this element.
    DuplicatedNamespace(String, TextPos),

    /// An unknown namespace.
    ///
    /// Indicates that an element or an attribute has an unknown qualified name prefix.
    ///
    /// The first value is a prefix.
    UnknownNamespace(String, TextPos),

    /// Incorrect tree structure.
    ///
    /// expected, actual, position
    UnexpectedCloseTag(String, String, TextPos),

    /// Entity value starts with a close tag.
    ///
    /// Example:
    /// ```xml
    /// <!DOCTYPE test [ <!ENTITY p '</p>'> ]>
    /// <root>&p;</root>
    /// ```
    UnexpectedEntityCloseTag(TextPos),

    /// A reference to an entity that was not defined in the DTD.
    UnknownEntityReference(String, TextPos),

    /// A malformed entity reference.
    ///
    /// A `&` character inside an attribute value or text indicates an entity reference.
    /// Otherwise, the document is not well-formed.
    MalformedEntityReference(TextPos),

    /// A possible entity reference loop.
    ///
    /// The current depth limit is 10. The max number of references per reference is 255.
    EntityReferenceLoop(TextPos),

    /// Attribute value cannot have a `<` character.
    InvalidAttributeValue(TextPos),

    /// An element has a duplicated attributes.
    ///
    /// This also includes namespaces resolving.
    /// So an element like this will lead to an error.
    /// ```xml
    /// <e xmlns:n1='http://www.w3.org' xmlns:n2='http://www.w3.org' n1:a='b1' n2:a='b2'/>
    /// ```
    DuplicatedAttribute(String, TextPos),

    /// The XML document must have at least one element.
    NoRootNode,

    /// The root node was opened but never closed.
    UnclosedRootNode,

    /// An XML document can have only one XML declaration
    /// and it must be at the start of the document.
    UnexpectedDeclaration(TextPos),

    /// An XML with DTD detected.
    ///
    /// This error will be emitted only when `ParsingOptions::allow_dtd` is set to `false`.
    DtdDetected,

    /// Indicates that the [`ParsingOptions::nodes_limit`] was reached.
    NodesLimitReached,

    /// Indicates that too many attributes were parsed.
    AttributesLimitReached,

    /// Indicates that too many namespaces were parsed.
    NamespacesLimitReached,

    /// An invalid name.
    InvalidName(TextPos),

    /// A non-XML character has occurred.
    ///
    /// Valid characters are: <https://www.w3.org/TR/xml/#char32>
    NonXmlChar(char, TextPos),

    /// An invalid/unexpected character.
    ///
    /// expected, actual, position
    InvalidChar(u8, u8, TextPos),

    /// An invalid/unexpected character.
    ///
    /// expected, actual, position
    InvalidChar2(&'static str, u8, TextPos),

    /// An unexpected string.
    ///
    /// Contains what string was expected.
    InvalidString(&'static str, TextPos),

    /// An invalid ExternalID in the DTD.
    InvalidExternalID(TextPos),

    /// The given entity resolved yielded an errror.
    EntityResolver(TextPos, String),

    /// A comment cannot contain `--` or end with `-`.
    InvalidComment(TextPos),

    /// A Character Data node contains an invalid data.
    ///
    /// Currently, only `]]>` is not allowed.
    InvalidCharacterData(TextPos),

    /// An unknown token.
    UnknownToken(TextPos),

    /// The steam ended earlier than we expected.
    ///
    /// Should only appear on invalid input data.
    UnexpectedEndOfStream,
}

impl Error {
    /// Returns the error position.
    pub fn pos(&self) -> TextPos {
        match *self {
            #[cfg(feature = "notepads-markdown-resources")]
            Error::Resource(_) => TextPos::new(1, 1),
            Error::InvalidXmlPrefixUri(pos) => pos,
            Error::UnexpectedXmlUri(pos) => pos,
            Error::UnexpectedXmlnsUri(pos) => pos,
            Error::InvalidElementNamePrefix(pos) => pos,
            Error::DuplicatedNamespace(_, pos) => pos,
            Error::UnknownNamespace(_, pos) => pos,
            Error::UnexpectedCloseTag(_, _, pos) => pos,
            Error::UnexpectedEntityCloseTag(pos) => pos,
            Error::UnknownEntityReference(_, pos) => pos,
            Error::MalformedEntityReference(pos) => pos,
            Error::EntityReferenceLoop(pos) => pos,
            Error::InvalidAttributeValue(pos) => pos,
            Error::DuplicatedAttribute(_, pos) => pos,
            Error::NoRootNode => TextPos::new(1, 1),
            Error::UnclosedRootNode => TextPos::new(1, 1),
            Error::UnexpectedDeclaration(pos) => pos,
            Error::DtdDetected => TextPos::new(1, 1),
            Error::NodesLimitReached => TextPos::new(1, 1),
            Error::AttributesLimitReached => TextPos::new(1, 1),
            Error::NamespacesLimitReached => TextPos::new(1, 1),
            Error::InvalidName(pos) => pos,
            Error::NonXmlChar(_, pos) => pos,
            Error::InvalidChar(_, _, pos) => pos,
            Error::InvalidChar2(_, _, pos) => pos,
            Error::InvalidString(_, pos) => pos,
            Error::InvalidExternalID(pos) => pos,
            Error::EntityResolver(pos, _) => pos,
            Error::InvalidComment(pos) => pos,
            Error::InvalidCharacterData(pos) => pos,
            Error::UnknownToken(pos) => pos,
            Error::UnexpectedEndOfStream => TextPos::new(1, 1),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            #[cfg(feature = "notepads-markdown-resources")]
            Error::Resource(error) => write!(f, "{}", error),
            Error::InvalidXmlPrefixUri(pos) => {
                write!(f, "'xml' namespace prefix mapped to wrong URI at {}", pos)
            }
            Error::UnexpectedXmlUri(pos) => {
                write!(
                    f,
                    "the 'xml' namespace URI is used for not 'xml' prefix at {}",
                    pos
                )
            }
            Error::UnexpectedXmlnsUri(pos) => {
                write!(
                    f,
                    "the 'xmlns' URI is used at {}, but it must not be declared",
                    pos
                )
            }
            Error::InvalidElementNamePrefix(pos) => {
                write!(
                    f,
                    "the 'xmlns' prefix is used at {}, but it must not be",
                    pos
                )
            }
            Error::DuplicatedNamespace(ref name, pos) => {
                write!(f, "namespace '{}' at {} is already defined", name, pos)
            }
            Error::UnknownNamespace(ref name, pos) => {
                write!(f, "an unknown namespace prefix '{}' at {}", name, pos)
            }
            Error::UnexpectedCloseTag(ref expected, ref actual, pos) => {
                write!(
                    f,
                    "expected '{}' tag, not '{}' at {}",
                    expected, actual, pos
                )
            }
            Error::UnexpectedEntityCloseTag(pos) => {
                write!(f, "unexpected close tag at {}", pos)
            }
            Error::MalformedEntityReference(pos) => {
                write!(f, "malformed entity reference at {}", pos)
            }
            Error::UnknownEntityReference(ref name, pos) => {
                write!(f, "unknown entity reference '{}' at {}", name, pos)
            }
            Error::EntityReferenceLoop(pos) => {
                write!(f, "a possible entity reference loop is detected at {}", pos)
            }
            Error::InvalidAttributeValue(pos) => {
                write!(f, "unescaped '<' found at {}", pos)
            }
            Error::DuplicatedAttribute(ref name, pos) => {
                write!(f, "attribute '{}' at {} is already defined", name, pos)
            }
            Error::NoRootNode => {
                write!(f, "the document does not have a root node")
            }
            Error::UnclosedRootNode => {
                write!(f, "the root node was opened but never closed")
            }
            Error::UnexpectedDeclaration(pos) => {
                write!(f, "unexpected XML declaration at {}", pos)
            }
            Error::DtdDetected => {
                write!(f, "XML with DTD detected")
            }
            Error::NodesLimitReached => {
                write!(f, "nodes limit reached")
            }
            Error::AttributesLimitReached => {
                write!(f, "more than 2^32 attributes were parsed")
            }
            Error::NamespacesLimitReached => {
                write!(f, "more than 2^16 unique namespaces were parsed")
            }
            Error::InvalidName(pos) => {
                write!(f, "invalid name token at {}", pos)
            }
            Error::NonXmlChar(c, pos) => {
                write!(f, "a non-XML character {:?} found at {}", c, pos)
            }
            Error::InvalidChar(expected, actual, pos) => {
                write!(
                    f,
                    "expected '{}' not '{}' at {}",
                    *expected as char, *actual as char, pos
                )
            }
            Error::InvalidChar2(expected, actual, pos) => {
                write!(
                    f,
                    "expected {} not '{}' at {}",
                    expected, *actual as char, pos
                )
            }
            Error::InvalidString(expected, pos) => {
                write!(f, "expected '{}' at {}", expected, pos)
            }
            Error::InvalidExternalID(pos) => {
                write!(f, "invalid ExternalID at {}", pos)
            }
            Error::EntityResolver(pos, msg) => {
                write!(f, "entity resolver failed at {}: {}", pos, msg)
            }
            Error::InvalidComment(pos) => {
                write!(f, "comment at {} contains '--'", pos)
            }
            Error::InvalidCharacterData(pos) => {
                write!(f, "']]>' at {} is not allowed inside a character data", pos)
            }
            Error::UnknownToken(pos) => {
                write!(f, "unknown token at {}", pos)
            }
            Error::UnexpectedEndOfStream => {
                write!(f, "unexpected end of stream")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn description(&self) -> &str {
        "an XML parsing error"
    }
}

/// Parsing options.
pub struct ParsingOptions<'input> {
    /// Active externally prepaid resources. None preserves the upstream unbudgeted route.
    #[cfg(feature = "notepads-markdown-resources")]
    pub resources: Option<markdown_svg_budget::ResourcesRef>,
    /// Allow DTD parsing.
    ///
    /// When set to `false`, XML with DTD will cause an error.
    /// Empty DTD block is not an error.
    ///
    /// Currently, there is no option to simply skip DTD.
    /// Mainly because you will get `UnknownEntityReference` error later anyway.
    ///
    /// This flag is set to `false` by default for security reasons,
    /// but `roxmltree` still has checks for billion laughs attack,
    /// so this is just an extra security measure.
    ///
    /// Default: false
    pub allow_dtd: bool,

    /// Sets the maximum number of nodes to parse.
    ///
    /// Useful when dealing with random input to limit memory usage.
    ///
    /// Default: u32::MAX (no limit)
    pub nodes_limit: u32,

    /// Function to resolve external entities
    ///
    /// See [`EntityResolver`] for the signature
    /// and the expected behaviour.
    pub entity_resolver: Option<&'input EntityResolver<'input>>,
}

/// Function to resolve external entities
///
/// This function is passed the optional public ID
/// and the mandatory URI of the external entity.
///
/// It is expected to yield the content defining
/// the URI, possibly preceeded by a text declaration.
///
/// If it yields [`None`], the entity is discarded
/// and will yield errors if referenced during parsing.
///
/// Errors must be stringified so they can be propagated
/// via [`Error::EntityResolver`].
pub type EntityResolver<'input> =
    dyn Fn(Option<&str>, &str) -> core::result::Result<Option<&'input str>, String> + 'input;

impl Default for ParsingOptions<'_> {
    fn default() -> Self {
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: u32::MAX,
            entity_resolver: None,
            #[cfg(feature = "notepads-markdown-resources")]
            resources: None,
        }
    }
}

impl ParsingOptions<'_> {
    fn resources(&self) -> crate::storage::Resources {
        #[cfg(feature = "notepads-markdown-resources")]
        {
            self.resources.clone()
        }
        #[cfg(not(feature = "notepads-markdown-resources"))]
        {}
    }
}

impl fmt::Debug for ParsingOptions<'_> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entity_resolver = if self.entity_resolver.is_some() {
            "Some(..)"
        } else {
            "None"
        };

        fmt.debug_struct("ParsingOptions")
            .field("allow_dtd", &self.allow_dtd)
            .field("nodes_limit", &self.nodes_limit)
            .field("entity_resolver", &entity_resolver)
            .finish()
    }
}

struct TempAttributeData<'input> {
    prefix: &'input str,
    local: &'input str,
    value: StringStorage<'input>,
    range: Range<usize>,
    #[cfg(feature = "positions")]
    qname_len: u16,
    #[cfg(feature = "positions")]
    eq_len: u8,
}

impl<'input> Document<'input> {
    /// Parses the input XML string.
    ///
    /// We do not support `&[u8]` or `Reader` because the input must be an already allocated
    /// UTF-8 string.
    ///
    /// This is a shorthand for `Document::parse_with_options(data, ParsingOptions::default())`.
    ///
    /// # Examples
    ///
    /// ```
    /// let doc = roxmltree::Document::parse("<e/>").unwrap();
    /// assert_eq!(doc.descendants().count(), 2); // root node + `e` element node
    /// ```
    #[inline]
    pub fn parse(text: &'input str) -> Result<Self> {
        Self::parse_with_options(text, ParsingOptions::default())
    }

    /// Parses the input XML string using to selected options.
    ///
    /// We do not support `&[u8]` or `Reader` because the input must be an already allocated
    /// UTF-8 string.
    ///
    /// # Examples
    ///
    /// ```
    /// let opt = roxmltree::ParsingOptions::default();
    /// let doc = roxmltree::Document::parse_with_options("<e/>", opt).unwrap();
    /// assert_eq!(doc.descendants().count(), 2); // root node + `e` element node
    /// ```
    #[inline]
    pub fn parse_with_options(text: &'input str, opt: ParsingOptions<'input>) -> Result<Self> {
        parse(text, opt)
    }
}

struct Entity<'input> {
    name: &'input str,
    value: StrSpan<'input>,
}

#[derive(Clone, Copy)]
struct TagNameSpan<'input> {
    prefix: &'input str,
    name: &'input str,
    pos: usize,
    prefix_pos: usize,
}

impl<'input> TagNameSpan<'input> {
    #[inline]
    fn new_null() -> Self {
        Self {
            prefix: "",
            name: "",
            pos: 0,
            prefix_pos: 0,
        }
    }
}

/// An entity loop detector.
///
/// Limits:
/// - Entities depth is 10.
/// - Maximum number of entity references per entity reference is 255.
///
/// Basically, if a text or an attribute has an entity reference and this reference
/// has more than 10 nested references - this is an error.
///
/// This is useful for simple loops like:
///
/// ```text
/// <!ENTITY a '&b;'>
/// <!ENTITY b '&a;'>
/// ```
///
/// And, if a text or an attribute has an entity reference and it references more
/// than 255 references - this is an error.
///
/// This is useful for cases like billion laughs attack, where depth can be pretty small,
/// but the number of references is exponentially increasing:
///
/// ```text
/// <!ENTITY lol "lol">
/// <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
/// <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
/// <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
/// <!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">
/// ```
#[derive(Default)]
struct LoopDetector {
    /// References depth.
    depth: u8,
    /// Number of references resolved by the root reference.
    references: u8,
}

impl LoopDetector {
    #[inline]
    fn inc_depth(&mut self, stream: &Stream) -> Result<()> {
        if self.depth < 10 {
            self.depth += 1;
            Ok(())
        } else {
            Err(Error::EntityReferenceLoop(stream.gen_text_pos()))
        }
    }

    #[inline]
    fn dec_depth(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }

        // Reset references count after reaching zero depth.
        if self.depth == 0 {
            self.references = 0;
        }
    }

    #[inline]
    fn inc_references(&mut self, stream: &Stream) -> Result<()> {
        if self.depth == 0 {
            // Allow infinite amount of references at zero depth.
            Ok(())
        } else {
            if self.references == u8::MAX {
                return Err(Error::EntityReferenceLoop(stream.gen_text_pos()));
            }

            self.references += 1;
            Ok(())
        }
    }
}

struct Context<'input> {
    opt: ParsingOptions<'input>,
    namespace_start_idx: usize,
    #[cfg(feature = "notepads-markdown-resources")]
    attribute_count: u64,
    #[cfg(feature = "notepads-markdown-resources")]
    element_attribute_count: u64,
    #[cfg(feature = "notepads-markdown-resources")]
    event_credit: u64,
    current_attributes: XmlVec<TempAttributeData<'input>>,
    awaiting_subtree: XmlVec<NodeId>,
    parent_prefixes: XmlVec<&'input str>,
    entities: Vec<Entity<'input>>,
    after_text: XmlVec<StringStorage<'input>>,
    parent_id: NodeId,
    tag_name: TagNameSpan<'input>,
    loop_detector: LoopDetector,
    doc: Document<'input>,
}

impl<'input> Context<'input> {
    fn append_node(&mut self, kind: NodeKind<'input>, range: Range<usize>) -> Result<NodeId> {
        if self.doc.nodes.len() >= self.opt.nodes_limit as usize {
            #[cfg(feature = "notepads-markdown-resources")]
            if let Some(resources) = &self.opt.resources {
                return Err(Error::Resource(resources.record(
                    markdown_svg_budget::Error::new(
                        markdown_svg_budget::FailureKind::Quota,
                        markdown_svg_budget::LimitKind::XmlNodes,
                    ),
                )));
            }
            return Err(Error::NodesLimitReached);
        }

        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.opt.resources {
            resources
                .check_limit(
                    markdown_svg_budget::LimitKind::XmlNodes,
                    self.doc.nodes.len() as u64 + 1,
                    resources.limits().xml_nodes,
                )
                .map_err(Error::Resource)?;
        }
        #[cfg(not(feature = "positions"))]
        let _ = range;

        let new_child_id = NodeId::from(self.doc.nodes.len());

        let appending_element = matches!(kind, NodeKind::Element { .. });
        self.doc.nodes.push(NodeData {
            parent: Some(self.parent_id),
            prev_sibling: None,
            next_subtree: None,
            last_child: None,
            kind,
            #[cfg(feature = "positions")]
            range,
        })?;

        let last_child_id = self.doc.nodes[self.parent_id.get_usize()].last_child;
        self.doc.nodes[new_child_id.get_usize()].prev_sibling = last_child_id;
        self.doc.nodes[self.parent_id.get_usize()].last_child = Some(new_child_id);

        for id in &self.awaiting_subtree {
            self.doc.nodes[id.get_usize()].next_subtree = Some(new_child_id);
        }
        self.awaiting_subtree.clear();

        if !appending_element {
            self.awaiting_subtree
                .push(NodeId::from(self.doc.nodes.len() - 1))?;
        }

        Ok(new_child_id)
    }

    fn append_text(&mut self, text: StringStorage<'input>, range: Range<usize>) -> Result<()> {
        if self.after_text.is_empty() {
            self.append_node(NodeKind::Text(text.clone()), range)?;
        }
        self.after_text.push(text)?;
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn merge_text(&mut self) -> Result<()> {
        let mut merged = TextBuffer::new(self.opt.resources());
        for part in self.after_text.iter() {
            merged.push_str(part.as_str())?;
        }
        let merged = merged.finish()?;
        match &mut self.doc.nodes.last_mut().unwrap().kind {
            NodeKind::Text(text) => *text = merged,
            _ => unreachable!(),
        }
        Ok(())
    }

    #[inline]
    fn reset_after_text(&mut self) -> Result<()> {
        if self.after_text.is_empty() {
            return Ok(());
        }
        if self.after_text.len() > 1 {
            self.merge_text()?;
        }
        self.after_text.clear();
        Ok(())
    }
}

fn record_parse_error(
    #[allow(unused)] resources: &crate::storage::Resources,
    #[allow(unused)] error: &Error,
) {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = resources {
        use markdown_svg_budget::{Error as ResourceError, FailureKind, LimitKind};
        let error = match error {
            Error::Resource(error) => *error,
            Error::NodesLimitReached => ResourceError::new(FailureKind::Quota, LimitKind::XmlNodes),
            Error::AttributesLimitReached => {
                ResourceError::new(FailureKind::Quota, LimitKind::XmlAttributes)
            }
            Error::NamespacesLimitReached => {
                ResourceError::new(FailureKind::Quota, LimitKind::Namespaces)
            }
            _ => ResourceError::new(FailureKind::Decode, LimitKind::None),
        };
        resources.record(error);
    }
}

fn parse<'input>(text: &'input str, opt: ParsingOptions<'input>) -> Result<Document<'input>> {
    // Event-driven paid growth replaces input-character capacity guesses.
    let resources = opt.resources();
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = &resources {
        use markdown_svg_budget::{FailureKind, LimitKind};
        resources
            .check_limit(
                LimitKind::InputBytes,
                text.len() as u64,
                resources.limits().input_bytes,
            )
            .map_err(Error::Resource)?;
        resources
            .check_limit(
                LimitKind::XmlNodes,
                1,
                resources.limits().xml_nodes.min(opt.nodes_limit as u64),
            )
            .map_err(Error::Resource)?;
        if opt.allow_dtd || opt.entity_resolver.is_some() {
            resources.record(markdown_svg_budget::Error::new(
                FailureKind::Unsupported,
                LimitKind::None,
            ));
            return Err(Error::DtdDetected);
        }
        // The tokenizer prepays actual scanning in bounded quanta, including
        // long single tokens. This entry check precedes all DOM allocation.
        resources.checkpoint(0).map_err(Error::Resource)?;
    }

    // Init document.
    let mut doc = Document {
        text,
        nodes: XmlVec::new(resources.clone()),
        attributes: XmlVec::new(resources.clone()),
        namespaces: Namespaces::with_resources(resources.clone()),
    };

    // Add a root node.
    doc.nodes.push(NodeData {
        parent: None,
        prev_sibling: None,
        next_subtree: None,
        last_child: None,
        kind: NodeKind::Root,
        #[cfg(feature = "positions")]
        range: 0..text.len(),
    })?;

    doc.namespaces
        .push_ns(Some(NS_XML_PREFIX), StringStorage::Borrowed(NS_XML_URI))?;

    let allow_dtd = opt.allow_dtd;

    let mut ctx = Context {
        opt,
        namespace_start_idx: 1,
        #[cfg(feature = "notepads-markdown-resources")]
        attribute_count: 0,
        #[cfg(feature = "notepads-markdown-resources")]
        element_attribute_count: 0,
        #[cfg(feature = "notepads-markdown-resources")]
        event_credit: 0,
        current_attributes: XmlVec::new(resources.clone()),
        entities: Vec::new(),
        awaiting_subtree: XmlVec::new(resources.clone()),
        parent_prefixes: XmlVec::new(resources.clone()),
        after_text: XmlVec::new(resources.clone()),
        parent_id: NodeId::new(0),
        tag_name: TagNameSpan::new_null(),
        loop_detector: LoopDetector::default(),
        doc,
    };

    ctx.parent_prefixes.push("")?;
    if let Err(error) = tokenizer::parse(text, allow_dtd, &mut ctx) {
        record_parse_error(&resources, &error);
        return Err(error);
    }

    let mut doc = ctx.doc;
    if !doc.root().children().any(|n| n.is_element()) {
        let error = Error::NoRootNode;
        record_parse_error(&resources, &error);
        return Err(error);
    }

    if ctx.parent_prefixes.len() > 1 {
        let error = Error::UnclosedRootNode;
        record_parse_error(&resources, &error);
        return Err(error);
    }

    doc.nodes.shrink_to_fit();
    doc.attributes.shrink_to_fit();
    doc.namespaces.shrink_to_fit();

    Ok(doc)
}

impl<'input> tokenizer::XmlEvents<'input> for Context<'input> {
    fn resources(&self) -> crate::storage::Resources {
        self.opt.resources()
    }

    #[inline(always)]
    fn token(&mut self, token: tokenizer::Token<'input>) -> Result<()> {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.opt.resources {
            if self.event_credit == 0 {
                resources.checkpoint(256).map_err(Error::Resource)?;
                self.event_credit = 256;
            }
            self.event_credit -= 1;
        }
        match token {
            tokenizer::Token::ProcessingInstruction(target, value, range) => {
                self.reset_after_text()?;
                let pi = NodeKind::PI(PI { target, value });
                self.append_node(pi, range)?;
            }
            tokenizer::Token::Comment(text, range) => {
                self.reset_after_text()?;
                self.append_node(NodeKind::Comment(StringStorage::Borrowed(text)), range)?;
            }
            tokenizer::Token::EntityDeclaration(name, definition) => {
                self.entities.push(Entity {
                    name,
                    value: definition,
                });
            }
            tokenizer::Token::ElementStart(prefix, local, start) => {
                #[cfg(feature = "notepads-markdown-resources")]
                if let Some(resources) = &self.opt.resources {
                    resources
                        .check_limit(
                            markdown_svg_budget::LimitKind::XmlDepth,
                            self.parent_prefixes.len() as u64,
                            resources.limits().xml_depth,
                        )
                        .map_err(Error::Resource)?;
                    self.element_attribute_count = 0;
                }
                self.reset_after_text()?;

                if prefix == XMLNS {
                    let pos = self.doc.text_pos_at(start + 1);
                    return Err(Error::InvalidElementNamePrefix(pos));
                }

                self.tag_name = TagNameSpan {
                    prefix,
                    name: local,
                    pos: start,
                    prefix_pos: start + 1,
                };
            }
            tokenizer::Token::Attribute(range, qname_len, eq_len, prefix, local, value) => {
                process_attribute(range, qname_len, eq_len, prefix, local, value, self)?;
            }
            tokenizer::Token::ElementEnd(end, range) => {
                self.reset_after_text()?;
                process_element(end, range, self)?;
            }
            tokenizer::Token::Text(text, range) => {
                process_text(text, range, self)?;
            }
            tokenizer::Token::Cdata(text, range) => {
                process_cdata(text, range, self)?;
            }
        }

        Ok(())
    }

    fn resolve_entity(
        &mut self,
        pub_id: Option<&str>,
        uri: &str,
    ) -> core::result::Result<Option<&'input str>, String> {
        match &mut self.opt.entity_resolver {
            Some(entity_resolver) => entity_resolver(pub_id, uri),
            None => Ok(None),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_attribute<'input>(
    range: Range<usize>,
    qname_len: u16,
    eq_len: u8,
    prefix: &'input str,
    local: &'input str,
    value: StrSpan<'input>,
    ctx: &mut Context<'input>,
) -> Result<()> {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = &ctx.opt.resources {
        use markdown_svg_budget::LimitKind;
        let total = ctx.attribute_count.checked_add(1).ok_or_else(|| {
            Error::Resource(resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                LimitKind::XmlAttributes,
            )))
        })?;
        let element = ctx.element_attribute_count.checked_add(1).ok_or_else(|| {
            Error::Resource(resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                LimitKind::XmlElementAttributes,
            )))
        })?;
        resources
            .check_limit(
                LimitKind::XmlAttributes,
                total,
                resources.limits().xml_attributes,
            )
            .map_err(Error::Resource)?;
        resources
            .check_limit(
                LimitKind::XmlElementAttributes,
                element,
                resources.limits().xml_element_attributes,
            )
            .map_err(Error::Resource)?;
        ctx.attribute_count = total;
        ctx.element_attribute_count = element;
    }
    let value = normalize_attribute(value, ctx)?;

    if prefix == XMLNS {
        // The xmlns namespace MUST NOT be declared as the default namespace.
        if value.as_str() == NS_XMLNS_URI {
            let pos = ctx.doc.text_pos_at(range.start);
            return Err(Error::UnexpectedXmlnsUri(pos));
        }

        let is_xml_ns_uri = value.as_str() == NS_XML_URI;

        // The prefix 'xml' is by definition bound to the namespace name
        // http://www.w3.org/XML/1998/namespace.
        // It MUST NOT be bound to any other namespace name.
        if local == NS_XML_PREFIX {
            if !is_xml_ns_uri {
                let pos = ctx.doc.text_pos_at(range.start);
                return Err(Error::InvalidXmlPrefixUri(pos));
            }
        } else {
            // The xml namespace MUST NOT be bound to a non-xml prefix.
            if is_xml_ns_uri {
                let pos = ctx.doc.text_pos_at(range.start);
                return Err(Error::UnexpectedXmlUri(pos));
            }
        }

        // Check for duplicated namespaces.
        if ctx
            .doc
            .namespaces
            .exists(ctx.namespace_start_idx, Some(local))?
        {
            let pos = ctx.doc.text_pos_at(range.start);
            return Err(named_error(&ctx.opt.resources(), || {
                Error::DuplicatedNamespace(local.to_string(), pos)
            }));
        }

        // Xml namespace should not be added to the namespaces.
        if !is_xml_ns_uri {
            ctx.doc.namespaces.push_ns(Some(local), value)?;
        }
    } else if local == XMLNS {
        // The xml namespace MUST NOT be declared as the default namespace.
        if value.as_str() == NS_XML_URI {
            let pos = ctx.doc.text_pos_at(range.start);
            return Err(Error::UnexpectedXmlUri(pos));
        }

        // The xmlns namespace MUST NOT be declared as the default namespace.
        if value.as_str() == NS_XMLNS_URI {
            let pos = ctx.doc.text_pos_at(range.start);
            return Err(Error::UnexpectedXmlnsUri(pos));
        }

        ctx.doc.namespaces.push_ns(None, value)?;
    } else {
        #[cfg(not(feature = "positions"))]
        let _ = (qname_len, eq_len);

        ctx.current_attributes.push(TempAttributeData {
            prefix,
            local,
            value,
            range,
            #[cfg(feature = "positions")]
            qname_len,
            #[cfg(feature = "positions")]
            eq_len,
        })?;
    }

    Ok(())
}

fn process_element<'input>(
    end_token: tokenizer::ElementEnd<'input>,
    token_range: Range<usize>,
    ctx: &mut Context<'input>,
) -> Result<()> {
    if ctx.tag_name.name.is_empty() {
        // May occur in XML like this:
        // <!DOCTYPE test [ <!ENTITY p '</p>'> ]>
        // <root>&p;</root>

        if let tokenizer::ElementEnd::Close(..) = end_token {
            return Err(Error::UnexpectedEntityCloseTag(
                ctx.doc.text_pos_at(token_range.start),
            ));
        } else {
            unreachable!("should be already checked by the tokenizer");
        }
    }

    let namespaces = ctx.resolve_namespaces()?;
    ctx.namespace_start_idx = ctx.doc.namespaces.tree_order.len();

    let attributes = resolve_attributes(namespaces, ctx)?;

    match end_token {
        tokenizer::ElementEnd::Empty => {
            let tag_ns_idx = get_ns_idx_by_prefix(
                namespaces,
                ctx.tag_name.prefix_pos,
                ctx.tag_name.prefix,
                &ctx.doc,
                &ctx.opt.resources(),
            )?;
            let new_element_id = ctx.append_node(
                NodeKind::Element {
                    tag_name: ExpandedNameIndexed {
                        namespace_idx: tag_ns_idx,
                        local_name: ctx.tag_name.name,
                    },
                    attributes,
                    namespaces,
                },
                ctx.tag_name.pos..token_range.end,
            )?;
            ctx.awaiting_subtree.push(new_element_id)?;
        }
        tokenizer::ElementEnd::Close(prefix, local) => {
            let parent_node = &mut ctx.doc.nodes[ctx.parent_id.get_usize()];
            // should never panic as we start with the single prefix of the
            // root node and always push another one when changing the parent
            let parent_prefix = *ctx.parent_prefixes.last().unwrap();

            #[cfg(feature = "positions")]
            {
                parent_node.range.end = token_range.end;
            }

            if let NodeKind::Element { ref tag_name, .. } = parent_node.kind {
                if prefix != parent_prefix || local != tag_name.local_name {
                    let expected_local = tag_name.local_name;
                    let pos = ctx.doc.text_pos_at(token_range.start);
                    return Err(named_error(&ctx.opt.resources(), || {
                        Error::UnexpectedCloseTag(
                            gen_qname_string(parent_prefix, expected_local),
                            gen_qname_string(prefix, local),
                            pos,
                        )
                    }));
                }
            }
            ctx.awaiting_subtree.push(ctx.parent_id)?;

            if let Some(id) = parent_node.parent {
                ctx.parent_id = id;
                ctx.parent_prefixes.pop();
                debug_assert!(!ctx.parent_prefixes.is_empty());
            } else {
                // May occur in XML like this:
                // <!DOCTYPE test [ <!ENTITY p '<p></p></p>'> ]>
                // <p>&p;&p;

                return Err(Error::UnexpectedEntityCloseTag(
                    ctx.doc.text_pos_at(token_range.start),
                ));
            }
        }
        tokenizer::ElementEnd::Open => {
            let tag_ns_idx = get_ns_idx_by_prefix(
                namespaces,
                ctx.tag_name.prefix_pos,
                ctx.tag_name.prefix,
                &ctx.doc,
                &ctx.opt.resources(),
            )?;
            ctx.parent_id = ctx.append_node(
                NodeKind::Element {
                    tag_name: ExpandedNameIndexed {
                        namespace_idx: tag_ns_idx,
                        local_name: ctx.tag_name.name,
                    },
                    attributes,
                    namespaces,
                },
                ctx.tag_name.pos..token_range.end,
            )?;
            ctx.parent_prefixes.push(ctx.tag_name.prefix)?;
        }
    }

    Ok(())
}

impl Context<'_> {
    fn resolve_namespaces(&mut self) -> Result<ShortRange> {
        if let NodeKind::Element { ref namespaces, .. } =
            self.doc.nodes[self.parent_id.get_usize()].kind
        {
            let parent_ns = *namespaces;
            if self.namespace_start_idx == self.doc.namespaces.tree_order.len() {
                return Ok(parent_ns);
            }

            for i in parent_ns.to_urange() {
                if !self.doc.namespaces.exists(
                    self.namespace_start_idx,
                    self.doc
                        .namespaces
                        .get(self.doc.namespaces.tree_order[i])
                        .name,
                )? {
                    self.doc.namespaces.push_ref(i)?;
                }
            }
        }

        Ok((self.namespace_start_idx..self.doc.namespaces.tree_order.len()).into())
    }
}

fn resolve_attributes(namespaces: ShortRange, ctx: &mut Context) -> Result<ShortRange> {
    if ctx.current_attributes.is_empty() {
        return Ok(ShortRange::new(0, 0));
    }

    if ctx.doc.attributes.len() + ctx.current_attributes.len() >= u32::MAX as usize {
        return Err(Error::AttributesLimitReached);
    }

    let start_idx = ctx.doc.attributes.len();
    let mut work = crate::storage::NamespaceWork::new(ctx.opt.resources());

    for attr in ctx.current_attributes.drain(..) {
        let namespace_idx = if attr.prefix == NS_XML_PREFIX {
            // The prefix 'xml' is by definition bound to the namespace name
            // http://www.w3.org/XML/1998/namespace. This namespace is added
            // to the document on creation and is always element 0.
            Some(NamespaceIdx(0))
        } else if attr.prefix.is_empty() {
            // 'The namespace name for an unprefixed attribute name
            // always has no value.'
            None
        } else {
            get_ns_idx_by_prefix(
                namespaces,
                attr.range.start,
                attr.prefix,
                &ctx.doc,
                &ctx.opt.resources(),
            )?
        };

        let attr_name = ExpandedNameIndexed {
            namespace_idx,
            local_name: attr.local,
        };

        // Check for duplicated attributes.
        let expanded_name = attr_name.as_expanded_name(&ctx.doc);
        let mut duplicate = false;
        for previous in &ctx.doc.attributes[start_idx..] {
            work.event()?;
            let previous = previous.name.as_expanded_name(&ctx.doc);
            let same_namespace = match (previous.namespace(), expanded_name.namespace()) {
                (None, None) => true,
                (Some(a), Some(b)) => work.equal(a, b)?,
                _ => false,
            };
            if same_namespace && work.equal(previous.name(), expanded_name.name())? {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            work.finish()?;
            let pos = ctx.doc.text_pos_at(attr.range.start);
            return Err(named_error(&ctx.opt.resources(), || {
                Error::DuplicatedAttribute(attr.local.to_string(), pos)
            }));
        }

        ctx.doc.attributes.push(AttributeData {
            name: attr_name,
            value: attr.value,
            #[cfg(feature = "positions")]
            range: attr.range,
            #[cfg(feature = "positions")]
            qname_len: attr.qname_len,
            #[cfg(feature = "positions")]
            eq_len: attr.eq_len,
        })?;
    }

    work.finish()?;
    Ok((start_idx..ctx.doc.attributes.len()).into())
}

// Search only within prepaid bounded scanning credit; a match pays only the
// bytes actually inspected, rather than rounding every short value to 64KiB.
fn find_in_text(
    text: &str,
    #[allow(unused)] resources: &crate::storage::Resources,
    search: impl Fn(&[u8]) -> Option<usize>,
) -> Result<Option<usize>> {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = resources {
        let mut offset = 0;
        while offset < text.len() {
            let end = text
                .len()
                .min(offset + resources.scan_allowance().map_err(Error::Resource)?);
            let found = search(&text.as_bytes()[offset..end]);
            let examined = found.map_or(end - offset, |index| index + 1);
            resources
                .consume_scan_work(examined)
                .map_err(Error::Resource)?;
            if let Some(index) = found {
                return Ok(Some(offset + index));
            }
            offset = end;
        }
        return Ok(None);
    }
    Ok(search(text.as_bytes()))
}

// Detailed upstream errors may allocate input-sized names. The controlled
// route records its fixed decode error before evaluating that legacy builder.
fn named_error(
    #[allow(unused)] resources: &crate::storage::Resources,
    original: impl FnOnce() -> Error,
) -> Error {
    #[cfg(feature = "notepads-markdown-resources")]
    if let Some(resources) = resources {
        return Error::Resource(resources.record(markdown_svg_budget::Error::new(
            markdown_svg_budget::FailureKind::Decode,
            markdown_svg_budget::LimitKind::None,
        )));
    }
    original()
}

fn process_text<'input>(
    text: &'input str,
    range: Range<usize>,
    ctx: &mut Context<'input>,
) -> Result<()> {
    // Add text as is if it has only valid characters.
    if find_in_text(text, &ctx.opt.resources(), |bytes| {
        memchr2(b'&', b'\r', bytes)
    })?
    .is_none()
    {
        ctx.append_text(StringStorage::Borrowed(text), range)?;
        return Ok(());
    }

    let mut text_buffer = TextBuffer::new(ctx.opt.resources());
    let mut is_as_is = false; // TODO: explain
    let mut stream =
        Stream::from_substr(ctx.doc.text, range.clone()).with_resources(ctx.opt.resources());
    while !stream.at_end() {
        match parse_next_chunk(&mut stream, &ctx.entities, &ctx.opt.resources())? {
            NextChunk::Byte(c) => {
                if is_as_is {
                    text_buffer.push_raw(c)?;
                    is_as_is = false;
                } else {
                    text_buffer.push_from_text(c, stream.at_end())?;
                }
            }
            NextChunk::Char(c) => {
                for b in CharToBytes::new(c) {
                    if ctx.loop_detector.depth > 0 {
                        text_buffer.push_from_text(b, stream.at_end())?;
                    } else {
                        // Characters not from entity should be added as is.
                        // Not sure why... At least `lxml` produces the same result.
                        text_buffer.push_raw(b)?;
                        is_as_is = true;
                    }
                }
            }
            NextChunk::Text(fragment) => {
                is_as_is = false;

                if !text_buffer.is_empty() {
                    ctx.append_text(text_buffer.finish()?, range.clone())?;
                }

                ctx.loop_detector.inc_references(&stream)?;
                ctx.loop_detector.inc_depth(&stream)?;

                let text = if fragment.range().start == 0 {
                    fragment.as_str()
                } else {
                    ctx.doc.text
                };

                let mut stream =
                    Stream::from_substr(text, fragment.range()).with_resources(ctx.opt.resources());
                let prev_tag_name = ctx.tag_name;
                ctx.tag_name = TagNameSpan::new_null();
                tokenizer::parse_content(&mut stream, ctx)?;
                ctx.tag_name = prev_tag_name;
                text_buffer.clear();

                ctx.loop_detector.dec_depth();
            }
        }
    }

    if !text_buffer.is_empty() {
        ctx.append_text(text_buffer.finish()?, range)?;
    }

    Ok(())
}

// While the whole purpose of CDATA is to indicate to an XML library that this text
// has to be stored as is, carriage return (`\r`) is still has to be replaced with `\n`.
fn process_cdata<'input>(
    mut text: &'input str,
    range: Range<usize>,
    ctx: &mut Context<'input>,
) -> Result<()> {
    let mut pos = find_in_text(text, &ctx.opt.resources(), |bytes| memchr(b'\r', bytes))?;

    // Add text as is if it has only valid characters.
    if pos.is_none() {
        ctx.append_text(StringStorage::Borrowed(text), range)?;
        return Ok(());
    }

    let mut buf = TextBuffer::new(ctx.opt.resources());

    while let Some(pos1) = pos {
        let (line, rest) = text.split_at(pos1);

        buf.push_str(line)?;
        buf.push_raw(b'\n')?;

        text = if rest.as_bytes().get(1) == Some(&b'\n') {
            &rest[2..]
        } else {
            &rest[1..]
        };

        pos = find_in_text(text, &ctx.opt.resources(), |bytes| memchr(b'\r', bytes))?;
    }

    buf.push_str(text)?;

    ctx.append_text(buf.finish()?, range)?;
    Ok(())
}

enum NextChunk<'a> {
    Byte(u8),
    Char(char),
    Text(StrSpan<'a>),
}

fn parse_next_chunk<'a>(
    stream: &mut Stream<'a>,
    entities: &[Entity<'a>],
    resources: &crate::storage::Resources,
) -> Result<NextChunk<'a>> {
    debug_assert!(!stream.at_end());

    // Safe, because we already checked that stream is not at the end.
    // But we have an additional `debug_assert` above just in case.
    let c = stream.curr_byte_unchecked();

    // Check for character/entity references.
    if c == b'&' {
        let start = stream.pos();
        match stream.consume_reference() {
            Some(Reference::Char(ch)) => Ok(NextChunk::Char(ch)),
            Some(Reference::Entity(name)) => entities
                .iter()
                .find(|e| e.name == name)
                .map(|e| NextChunk::Text(e.value))
                .ok_or_else(|| {
                    let pos = stream.gen_text_pos_from(start);
                    named_error(resources, || {
                        Error::UnknownEntityReference(name.into(), pos)
                    })
                }),
            None => {
                let pos = stream.gen_text_pos_from(start);
                Err(Error::MalformedEntityReference(pos))
            }
        }
    } else {
        stream.scan_work(1)?;
        stream.advance(1);
        Ok(NextChunk::Byte(c))
    }
}

// https://www.w3.org/TR/REC-xml/#AVNormalize
fn normalize_attribute<'input>(
    text: StrSpan<'input>,
    ctx: &mut Context<'input>,
) -> Result<StringStorage<'input>> {
    // We assume that `&` indicates an entity or a character reference.
    // But in rare cases it can be just an another character.
    if find_in_text(text.as_str(), &ctx.opt.resources(), |bytes| {
        memchr2(b'&', b'\t', bytes)
    })?
    .is_some()
        || find_in_text(text.as_str(), &ctx.opt.resources(), |bytes| {
            memchr2(b'\n', b'\r', bytes)
        })?
        .is_some()
    {
        let mut text_buffer = TextBuffer::new(ctx.opt.resources());
        _normalize_attribute(text, &mut text_buffer, ctx)?;
        text_buffer.finish()
    } else {
        Ok(StringStorage::Borrowed(text.as_str()))
    }
}

fn _normalize_attribute(text: StrSpan, buffer: &mut TextBuffer, ctx: &mut Context) -> Result<()> {
    let mut stream =
        Stream::from_substr(ctx.doc.text, text.range()).with_resources(ctx.opt.resources());
    while !stream.at_end() {
        // Safe, because we already checked that the stream is not at the end.
        let c = stream.curr_byte_unchecked();

        if c != b'&' {
            stream.scan_work(1)?;
            stream.advance(1);
            buffer.push_from_attr(c, stream.curr_byte().ok())?;
            continue;
        }

        // Check for character/entity references.
        let start = stream.pos();
        match stream.consume_reference() {
            Some(Reference::Char(ch)) => {
                for b in CharToBytes::new(ch) {
                    if ctx.loop_detector.depth > 0 {
                        // Escaped `<` inside an ENTITY is an error.
                        // Escaped `<` outside an ENTITY is ok.
                        if b == b'<' {
                            return Err(Error::InvalidAttributeValue(
                                stream.gen_text_pos_from(start),
                            ));
                        }

                        buffer.push_from_attr(b, None)?;
                    } else {
                        // Characters not from entity should be added as is.
                        // Not sure why... At least `lxml` produces the same results.
                        buffer.push_raw(b)?;
                    }
                }
            }
            Some(Reference::Entity(name)) => match ctx.entities.iter().find(|e| e.name == name) {
                Some(entity) => {
                    ctx.loop_detector.inc_references(&stream)?;
                    ctx.loop_detector.inc_depth(&stream)?;
                    _normalize_attribute(entity.value, buffer, ctx)?;
                    ctx.loop_detector.dec_depth();
                }
                None => {
                    let pos = stream.gen_text_pos_from(start);
                    return Err(named_error(&ctx.opt.resources(), || {
                        Error::UnknownEntityReference(name.into(), pos)
                    }));
                }
            },
            None => {
                let pos = stream.gen_text_pos_from(start);
                return Err(Error::MalformedEntityReference(pos));
            }
        }
    }

    Ok(())
}

fn get_ns_idx_by_prefix(
    namespaces: ShortRange,
    prefix_pos: usize,
    prefix: &str,
    doc: &Document<'_>,
    resources: &crate::storage::Resources,
) -> Result<Option<NamespaceIdx>> {
    // Prefix CAN be empty when the default namespace was defined.
    //
    // Example:
    // <e xmlns='http://www.w3.org'/>
    let prefix_opt = if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    };

    let mut work = crate::storage::NamespaceWork::new(resources.clone());
    let mut idx = None;
    for candidate in &doc.namespaces.tree_order[namespaces.to_urange()] {
        work.event()?;
        if work.name(doc.namespaces.get(*candidate).name, prefix_opt)? == core::cmp::Ordering::Equal
        {
            idx = Some(candidate);
            break;
        }
    }
    work.finish()?;

    match idx {
        Some(idx) => Ok(Some(*idx)),
        None => {
            if !prefix.is_empty() {
                // If an URI was not found and prefix IS NOT empty than
                // we have an unknown namespace.
                //
                // Example:
                // <e random:a='b'/>
                let pos = doc.text_pos_at(prefix_pos);
                Err(named_error(resources, || {
                    Error::UnknownNamespace(prefix.to_string(), pos)
                }))
            } else {
                // If an URI was not found and prefix IS empty than
                // an element or an attribute doesn't have a namespace.
                //
                // Example:
                // <e a='b'/>
                Ok(None)
            }
        }
    }
}

fn gen_qname_string(prefix: &str, local: &str) -> String {
    if prefix.is_empty() {
        local.to_string()
    } else {
        alloc::format!("{}:{}", prefix, local)
    }
}

/// Iterate over `char` by `u8`.
struct CharToBytes {
    buf: [u8; 4],
    idx: u8,
}

impl CharToBytes {
    #[inline]
    fn new(c: char) -> Self {
        let mut buf = [0xFF; 4];
        c.encode_utf8(&mut buf);

        CharToBytes { buf, idx: 0 }
    }
}

impl Iterator for CharToBytes {
    type Item = u8;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx < 4 {
            let b = self.buf[self.idx as usize];

            if b != 0xFF {
                self.idx += 1;
                return Some(b);
            } else {
                self.idx = 4;
            }
        }

        None
    }
}

// A fixed staging buffer preserves UTF-8 and the byte-level XML CR rules while
// avoiding one platform checkpoint per output byte. Only complete UTF-8 prefixes
// enter XmlText/PaidUtf8; unfinished code points and a trailing CR stay here.
const TEXT_STAGING_BYTES: usize = 4096;
struct TextBuffer {
    buffer: XmlText,
    staging: [u8; TEXT_STAGING_BYTES],
    staged: usize,
    resources: crate::storage::Resources,
}

impl TextBuffer {
    #[inline]
    fn new(resources: crate::storage::Resources) -> Self {
        Self {
            buffer: XmlText::new(resources.clone()),
            staging: [0; TEXT_STAGING_BYTES],
            staged: 0,
            resources,
        }
    }

    fn invalid_utf8(&self) -> Error {
        #[cfg(feature = "notepads-markdown-resources")]
        if let Some(resources) = &self.resources {
            return Error::Resource(resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Invariant,
                markdown_svg_budget::LimitKind::None,
            )));
        }
        // Input is str and normalization only changes ASCII. Reaching this
        // branch is an internal invariant failure, not valid input fallback.
        Error::InvalidString("valid normalized UTF-8", TextPos::new(1, 1))
    }

    fn flush(&mut self, final_chunk: bool) -> Result<()> {
        if self.staged == 0 {
            return Ok(());
        }
        let mut end = match core::str::from_utf8(&self.staging[..self.staged]) {
            Ok(_) => self.staged,
            Err(error) if !final_chunk && error.error_len().is_none() => error.valid_up_to(),
            Err(_) => return Err(self.invalid_utf8()),
        };
        if !final_chunk && end != 0 && self.staging[end - 1] == b'\r' {
            end -= 1;
        }
        if end == 0 {
            return if final_chunk || self.staged == TEXT_STAGING_BYTES {
                Err(self.invalid_utf8())
            } else {
                Ok(())
            };
        }
        let text = core::str::from_utf8(&self.staging[..end]).map_err(|_| self.invalid_utf8())?;
        self.buffer.push_str(text)?;
        self.staging.copy_within(end..self.staged, 0);
        self.staged -= end;
        Ok(())
    }

    #[inline]
    fn push_raw(&mut self, byte: u8) -> Result<()> {
        if self.staged == TEXT_STAGING_BYTES {
            self.flush(false)?;
        }
        self.staging[self.staged] = byte;
        self.staged += 1;
        Ok(())
    }

    fn push_str(&mut self, text: &str) -> Result<()> {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            if self.staged == TEXT_STAGING_BYTES {
                self.flush(false)?;
            }
            let count = bytes.len().min(TEXT_STAGING_BYTES - self.staged);
            self.staging[self.staged..self.staged + count].copy_from_slice(&bytes[..count]);
            self.staged += count;
            bytes = &bytes[count..];
        }
        Ok(())
    }

    fn push_from_attr(&mut self, current: u8, next: Option<u8>) -> Result<()> {
        if current == b'\r' && next == Some(b'\n') {
            return Ok(());
        }
        self.push_raw(match current {
            b'\n' | b'\r' | b'\t' => b' ',
            _ => current,
        })
    }

    fn push_from_text(&mut self, byte: u8, at_end: bool) -> Result<()> {
        if self.staged != 0 && self.staging[self.staged - 1] == b'\r' {
            self.staging[self.staged - 1] = b'\n';
            if at_end && byte == b'\r' {
                self.push_raw(b'\n')?;
            } else if byte != b'\n' {
                self.push_raw(byte)?;
            }
        } else if at_end && byte == b'\r' {
            self.push_raw(b'\n')?;
        } else {
            self.push_raw(byte)?;
        }
        Ok(())
    }

    #[inline]
    fn clear(&mut self) {
        self.buffer.clear();
        self.staged = 0;
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.buffer.is_empty() && self.staged == 0
    }

    fn finish<'input>(&mut self) -> Result<StringStorage<'input>> {
        self.flush(true)?;
        let next = XmlText::new(self.resources.clone());
        core::mem::replace(&mut self.buffer, next).into_storage()
    }
}
