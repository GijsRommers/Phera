//! Apply libxml's legacy HTML line breaks when formatOutput is enabled.
//!
//! ```php
//! $document->formatOutput = true;
//! echo $document->saveHTML();
//! ```

use crate::tree::{Kind, NodeId};

use super::{Action, Serializer, markup::html_block};

impl Serializer<'_> {
    pub(super) fn formatted_html_children(
        &self,
        id: NodeId,
        depth: usize,
        pending: &mut Vec<Action>,
        output: &mut String,
    ) {
        let node = &self.arena[id];
        let block = html_block(&node.name.local)
            && node.name.local != "p"
            && node.first_child() != node.last_child();
        let text = |id: NodeId| matches!(self.arena[id].kind, Kind::Text | Kind::EntityReference);
        pending.push(Action::Close {
            id,
            depth,
            indent: false,
        });
        if block && node.last_child().is_some_and(|id| !text(id)) {
            pending.push(Action::Newline);
        }
        let mut child = node.last_child();
        while let Some(id) = child {
            let entry = &self.arena[id];
            child = entry.previous_sibling();
            if block
                && entry.kind == Kind::Element
                && html_block(&entry.name.local)
                && entry.next_sibling().is_some_and(|id| !text(id))
            {
                pending.push(Action::Newline);
            }
            pending.push(Action::Visit {
                id,
                depth,
                pretty: true,
                indent: false,
            });
        }
        if block && node.first_child().is_some_and(|id| !text(id)) {
            output.push('\n');
        }
    }
}
