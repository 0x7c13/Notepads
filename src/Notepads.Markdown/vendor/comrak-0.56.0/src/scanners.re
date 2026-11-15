/*!re2c
    re2c:case-insensitive    = 1;

    re2c:sentinel            = 255;
    re2c:define:YYCTYPE      = u8;
    re2c:define:YYPEEK       = "if cursor < len { *s.as_bytes().get_unchecked(cursor) } else { 255 }";
    re2c:define:YYSKIP       = "cursor += 1;";
    re2c:define:YYBACKUP     = "marker = cursor;";
    re2c:define:YYRESTORE    = "cursor = marker;";
    re2c:define:YYBACKUPCTX  = "ctxmarker = cursor;";
    re2c:define:YYRESTORECTX = "cursor = ctxmarker;";
    re2c:yyfill:enable       = 0;
    re2c:indent:string       = '    ';
    re2c:indent:top          = 1;

    wordchar = [^\x01-\x20\xff];

    spacechar = [ \t\v\f\r\n];

    reg_char     = [^\\()\x01-\x20\xff];

    escaped_char = [\\][!"#$%&'()*+,./:;<=>?@[\\\]^_`{|}~-];

    tagname = [A-Za-z][A-Za-z0-9-]*;

    blocktagname = 'address'|'article'|'aside'|'base'|'basefont'|'blockquote'|'body'|'caption'|'center'|'col'|'colgroup'|'dd'|'details'|'dialog'|'dir'|'div'|'dl'|'dt'|'fieldset'|'figcaption'|'figure'|'footer'|'form'|'frame'|'frameset'|'h1'|'h2'|'h3'|'h4'|'h5'|'h6'|'head'|'header'|'hr'|'html'|'iframe'|'legend'|'li'|'link'|'main'|'menu'|'menuitem'|'nav'|'noframes'|'ol'|'optgroup'|'option'|'p'|'param'|'search'|'section'|'title'|'summary'|'table'|'tbody'|'td'|'tfoot'|'th'|'thead'|'title'|'tr'|'track'|'ul';

    attributename = [a-zA-Z_:][a-zA-Z0-9:._-]*;

    unquotedvalue = [^ \t\r\n\v\f"'=<>`\xff]+;
    singlequotedvalue = ['][^'\xff]*['];
    doublequotedvalue = ["][^"\xff]*["];

    attributevalue = unquotedvalue | singlequotedvalue | doublequotedvalue;

    attributevaluespec = spacechar* [=] spacechar* attributevalue;

    attribute = spacechar+ attributename attributevaluespec?;

    opentag = tagname attribute* spacechar* [/]? [>];
    closetag = [/] tagname spacechar* [>];

    htmlcomment = "--" ([^\xff-]+ | "-" [^\xff-] | "--" [^\xff>])* "-->";

    processinginstruction = ([^?>\xff]+ | [?][^>\xff] | [>])+;

    declaration = [A-Z]+ spacechar+ [^>\xff]*;

    cdata = "CDATA[" ([^\]\xff]+ | "]" [^\]\xff] | "]]" [^>\xff])*;

    htmltag = opentag | closetag;

    in_parens_nosp   = [(] (reg_char|escaped_char|[\\])* [)];

    in_double_quotes = ["] (escaped_char|[^"\xff])* ["];
    in_single_quotes = ['] (escaped_char|[^'\xff])* ['];
    in_parens        = [(] (escaped_char|[^)\xff])* [)];

    scheme           = [A-Za-z][A-Za-z0-9.+-]{1,31};

    /* Phoenix Tags */
    /* https://github.com/phoenixframework/tree-sitter-heex/blob/6603380caf806b3e6c7f0bf61627bb47023d79f1/grammar.js */
    phoenix_function_component = [.][a-z] [^-<>{}!"'/= \t\r\n\v\f.\xff]*;
    phoenix_module_component = [A-Z] [^-<>{}!"'/= \t\r\n\v\f.\xff]* ([.][A-Z] [^-<>{}!"'/= \t\r\n\v\f.\xff]*)* ([.][a-z] [^-<>{}!"'/= \t\r\n\v\f.\xff]*)?;
    phoenix_slot = [:][a-z]+ [^<>{}!"'/= \t\r\n\v\f\xff]*;
    phoenix_tag = phoenix_function_component | phoenix_module_component | phoenix_slot;
    phoenix_block_tag = phoenix_function_component | phoenix_module_component;
*/

pub fn open_block_directive_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [:]{3,} / [^:\r\n\xff]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

pub fn close_block_directive_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [:]{3,} / [ \t]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

pub fn atx_heading_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [#]{1,6} ([ \t]+|[\r\n\xff])  {
        if cursor == len + 1 {
            cursor -= 1;
        }
        return Some(cursor);
    }
    * { return None; }
*/
}

pub fn atx_subtext_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [-][#] ([ \t]+|[\r\n\xff])  {
        if cursor == len + 1 {
            cursor -= 1;
        }
        return Some(cursor);
    }
    * { return None; }
*/
}

pub fn html_block_end_1(s: &str) -> bool {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [^\n\xff]* [<] [/] ('script'|'pre'|'textarea'|'style') [>] { return true; }
    * { return false; }
*/
}

pub fn html_block_end_2(s: &str) -> bool {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [^\n\xff]* '-->' { return true; }
    * { return false; }
*/
}

pub fn html_block_end_3(s: &str) -> bool {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [^\n\xff]* '?>' { return true; }
    * { return false; }
*/
}

pub fn html_block_end_4(s: &str) -> bool {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [^\n\xff]* '>' { return true; }
    * { return false; }
*/
}

pub fn html_block_end_5(s: &str) -> bool {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [^\n\xff]* ']]>' { return true; }
    * { return false; }
*/
}

pub fn alert_start(s: &str) -> Option<crate::nodes::AlertType> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();

/*!re2c
    [>]{1,} ' [!note]' { return Some(crate::nodes::AlertType::Note); }
    [>]{1,} ' [!tip]' { return Some(crate::nodes::AlertType::Tip); }
    [>]{1,} ' [!important]' { return Some(crate::nodes::AlertType::Important); }
    [>]{1,} ' [!warning]' { return Some(crate::nodes::AlertType::Warning); }
    [>]{1,} ' [!caution]' { return Some(crate::nodes::AlertType::Caution); }
    * { return None; }
*/
}

pub fn open_code_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [`]{3,} / [^`\r\n\xff]*[\r\n\xff] { return Some(cursor); }
    [~]{3,} / [^\r\n\xff]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

pub fn close_code_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [`]{3,} / [ \t]*[\r\n\xff] { return Some(cursor); }
    [~]{3,} / [ \t]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_block_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [<] ('script'|'pre'|'textarea'|'style') (spacechar | [>]) { return Some(1); }
    '<!--' { return Some(2); }
    '<?' { return Some(3); }
    '<!' [A-Za-z] { return Some(4); }
    '<![CDATA[' { return Some(5); }
    [<] [/]? blocktagname (spacechar | [/]? [>])  { return Some(6); }
    * { return None; }
*/
}

pub fn html_block_start_7(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [<] (opentag | closetag) [\t\n\f ]* [\r\n\xff] { return Some(7); }
    * { return None; }
*/
}

pub enum SetextChar {
    Equals,
    Hyphen,
}

pub fn setext_heading_line(s: &str) -> Option<SetextChar> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [=]+ [ \t]* [\r\n\xff] { return Some(SetextChar::Equals); }
    [-]+ [ \t]* [\r\n\xff] { return Some(SetextChar::Hyphen); }
    * { return None; }
*/
}

pub fn footnote_definition(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    '[^' ([^\] \r\n\xff\t]+) ']:' [ \t]* { return Some(cursor); }
    * { return None; }
*/
}

pub fn scheme(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    scheme [:] { return Some(cursor); }
    * { return None; }
*/
}

pub fn autolink_uri(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    scheme [:][^\x01-\x20\xff<>]*[>]  { return Some(cursor); }
    * { return None; }
*/
}

pub fn autolink_email(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+
        [@]
        [a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?
        ([.][a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*
        [>] { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_tag(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    htmltag { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_comment(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    htmlcomment { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_processing_instruction(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    processinginstruction { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_declaration(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    declaration { return Some(cursor); }
    * { return None; }
*/
}

pub fn html_cdata(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    cdata { return Some(cursor); }
    * { return None; }
*/
}

pub fn spacechars(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let len = s.len();
/*!re2c
    [ \t\v\f\r\n]+ { return Some(cursor); }
    * { return None; }
*/
}

pub fn link_title(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    ["] (escaped_char|[^"\xff])* ["]   { return Some(cursor); }
    ['] (escaped_char|[^'\xff])* ['] { return Some(cursor); }
    [(] (escaped_char|[^()\xff])* [)]  { return Some(cursor); }
    * { return None; }
*/
}

pub fn dangerous_url(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    'data:image/' ('png'|'gif'|'jpeg'|'webp') { return None; }
    'javascript:' | 'vbscript:' | 'file:' | 'data:' { return Some(cursor); }
    * { return None; }
*/
}

pub fn ipv6_url_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    'http' [s]? '://[' [0-9a-fA-F:]+ ('%25' [a-zA-Z0-9]+)? ']' { return Some(cursor); }
    * { return None; }
*/
}

pub fn ipv6_relaxed_url_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [a-z]+ '://[' [0-9a-fA-F:]+ ('%25' [a-zA-Z0-9]+)? ']' { return Some(cursor); }
    * { return None; }
*/
}

/*!re2c

    table_spoiler = ['|']['|'];
    table_spacechar = [ \t\v\f];
    table_newline = ([\r][\n]|[\r\n]);

    table_delimiter = (table_spacechar*[:]?[-]+[:]?table_spacechar*);
    table_cell = (escaped_char|[^\xff|\r\n])+;
    table_cell_spoiler = (escaped_char|table_spoiler|[^\xff|\r\n])+;

*/

pub fn table_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [|]? table_delimiter ([|] table_delimiter)* [|]? table_spacechar* (table_newline|[\xff]) {
        return Some(cursor);
    }
    * { return None; }
*/
}

pub fn table_cell(s: &str, spoiler: bool) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();

    // In fact, `table_cell` matches non-empty table cells only. The empty
    // string is also a valid table cell, but is handled by the default rule.
    // This approach prevents re2c's match-empty-string warning.
    if spoiler {
/*!re2c
    table_cell_spoiler { return Some(cursor); }
    * { return None; }
*/
    } else {
/*!re2c
    table_cell { return Some(cursor); }
    * { return None; }
*/
    }
}

pub fn table_cell_end(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let len = s.len();
/*!re2c
    [|] table_spacechar* { return Some(cursor); }
    * { return None; }
*/
}

pub fn table_row_end(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    table_spacechar* table_newline { return Some(cursor); }
    * { return None; }
*/
}

#[cfg(feature = "shortcodes")]
pub fn shortcode(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [A-Za-z0-9+_-]+ [:] { return Some(cursor); }
    * { return None; }
*/
}

pub fn open_multiline_block_quote_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [>]{3,} / [ \t]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

pub fn close_multiline_block_quote_fence(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let mut ctxmarker = 0;
    let len = s.len();
/*!re2c
    [>]{3,} / [ \t]*[\r\n\xff] { return Some(cursor); }
    * { return None; }
*/
}

// Returns the length of the match, the tasklist item contents, and the range of
// the tasklist item match.
// The match is not guaranteed to be one byte or one "character" long; the
// caller must ascertain its fitness for purpose.
pub fn tasklist(s: &str) -> Option<(usize, &str, std::ops::Range<usize>)> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();

    let t1;
    let mut t2;
/*!stags:re2c format = 'let mut @@{tag} = 0;'; */

/*!local:re2c
    re2c:define:YYSTAGP = "@@{tag} = cursor;";
    re2c:define:YYSHIFTSTAG = "@@{tag} = (@@{tag} as isize + @@{shift}) as usize;";
    re2c:tags = 2;

    spacechar* [[] @t1 [^\xff\r\n\]]+ @t2 [\]] (spacechar | [\xff]) {
        if cursor == len + 1 {
            cursor -= 1;
        }
        return Some((cursor, &s[t1..t2], t1..t2));
    }
    * { return None; }
*/
}

pub fn description_item_start(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let len = s.len();
/*!re2c
    [:~] ([ \t]+) { return Some(cursor); }
    * { return None; }
*/
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_opening_tag(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [<] phoenix_block_tag { return Some(cursor - 1); }
    * { return None; }
*/
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_closing_tag(s: &str) -> Option<usize> {
    let mut cursor = 0;
    let mut marker = 0;
    let len = s.len();
/*!re2c
    [<][/] phoenix_block_tag [>] { return Some(cursor - 3); }
    * { return None; }
*/
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_closing_tag(s: &str) -> Option<usize> {
    phoenix_block_closing_tag(s).map(|tag_len| tag_len + 3)
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_closing_tag_end(s: &str, tag_name: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i + 3 < len {
        if bytes[i] == b'<'
            && bytes[i + 1] == b'/'
            && let Some(tag_name_len) = phoenix_block_closing_tag(&s[i..])
            && &s[i + 2..i + 2 + tag_name_len] == tag_name
        {
            return Some(i + 2 + tag_name_len + 1);
        }
        i += 1;
    }
    None
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_self_closing_tag_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 4 || bytes[0] != b'<' {
        return None;
    }

    let mut i = 1;
    while i + 1 < len {
        match bytes[i] {
            b'"' | b'\'' => {
                i = crate::strings::skip_quoted_string(s, i + 1, bytes[i]);
            }
            b'/' if bytes[i + 1] == b'>' => {
                return Some(i + 2);
            }
            b'>' => {
                return None;
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_directive(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let len = s.len();

    if s.len() < 4 || bytes[0] != b'<' || bytes[1] != b'%' {
        return None;
    }

    if bytes[2] == b'!' && s.len() >= 7 {
        let mut cursor = 0;
        let mut marker = 0;
/*!re2c
        '<%!--' ([^\xff-] | '-' [^\xff-] | '--' [^\xff%])* '--%>' { return Some(cursor); }
        * { return None; }
*/
    }

    if bytes[2] == b'#' {
        let mut cursor = 0;
        let mut marker = 0;
/*!re2c
        '<%#' ([^\xff%] | '%' [^\xff>])* '%>' { return Some(cursor); }
        * { return None; }
*/
    }

    let mut cursor = 2;
    while cursor + 1 < len {
        match bytes[cursor] {
            b'"' | b'\'' => {
                cursor = crate::strings::skip_quoted_string(s, cursor + 1, bytes[cursor]);
            }
            b'%' if bytes[cursor + 1] == b'>' => {
                return Some(cursor + 2);
            }
            _ => cursor += 1,
        }
    }

    None
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_directive_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'<' && bytes[1] == b'%' {
        Some(2)
    } else {
        None
    }
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_expression_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if !bytes.is_empty() && bytes[0] == b'{' {
        Some(1)
    } else {
        None
    }
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_directive_end(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 2 {
        return false;
    }

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                i = crate::strings::skip_quoted_string(s, i + 1, bytes[i]);
            }
            b'-' if i + 3 < len && bytes[i + 1] == b'-' && bytes[i + 2] == b'%' && bytes[i + 3] == b'>' => {
                return true;
            }
            b'%' if i + 1 < len && bytes[i + 1] == b'>' => {
                return true;
            }
            _ => i += 1,
        }
    }

    false
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_comment_end(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 2 {
        return false;
    }

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                i = crate::strings::skip_quoted_string(s, i + 1, bytes[i]);
            }
            b'%' if i + 1 < len && bytes[i + 1] == b'>' => {
                return true;
            }
            _ => i += 1,
        }
    }

    false
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_multiline_comment_end(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 4 {
        return false;
    }

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                i = crate::strings::skip_quoted_string(s, i + 1, bytes[i]);
            }
            b'-' if i + 3 < len && bytes[i + 1] == b'-' && bytes[i + 2] == b'%' && bytes[i + 3] == b'>' => {
                return true;
            }
            _ => i += 1,
        }
    }

    false
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_block_expression_end(s: &str) -> bool {
    let bytes = s.as_bytes();
    let len = bytes.len();

    if len < 1 {
        return false;
    }

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'"' | b'\'' => {
                i = crate::strings::skip_quoted_string(s, i + 1, bytes[i]);
            }
            b'}' => {
                return true;
            }
            _ => i += 1,
        }
    }

    false
}
