//! Legacy markup spellings used by XML and HTML serializers.
//!
//! ```php
//! echo $document->saveHTML($document->documentElement);
//! ```

use crate::tree::Node;
use std::fmt::Write;

pub(super) fn push_name(output: &mut String, node: &Node) {
    if let Some(prefix) = &node.name.prefix {
        output.push_str(prefix);
        output.push(':');
    }
    output.push_str(&node.name.local);
}

pub(super) fn write_doctype(node: &Node, output: &mut String) {
    let _ = write!(output, "<!DOCTYPE {}", node.name.local);
    if let Some(doctype) = &node.doctype {
        if !doctype.public_id.is_empty() {
            let _ = write!(output, " PUBLIC \"{}\"", doctype.public_id);
        }
        if !doctype.system_id.is_empty() {
            if doctype.public_id.is_empty() {
                output.push_str(" SYSTEM");
            }
            let _ = write!(output, " \"{}\"", doctype.system_id);
        }
        if !doctype.notations.is_empty() || doctype.internal_subset.is_some() {
            let subset = doctype.internal_subset.as_deref().unwrap_or_default();
            let _ = write!(output, " [\n{}{subset}]", doctype.notations);
        }
    }
    output.push('>');
}

pub(super) fn html_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "br"
            | "col"
            | "embed"
            | "frame"
            | "hr"
            | "img"
            | "input"
            | "isindex"
            | "keygen"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

pub(super) fn html_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "base"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "hr"
            | "html"
            | "isindex"
            | "li"
            | "link"
            | "menu"
            | "meta"
            | "noframes"
            | "noscript"
            | "ol"
            | "p"
            | "pre"
            | "script"
            | "style"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "ul"
    )
}

pub(super) fn html_boolean(name: &str) -> bool {
    matches!(
        name,
        "checked"
            | "compact"
            | "declare"
            | "defer"
            | "disabled"
            | "ismap"
            | "multiple"
            | "nohref"
            | "noresize"
            | "noshade"
            | "nowrap"
            | "readonly"
            | "selected"
    )
}
