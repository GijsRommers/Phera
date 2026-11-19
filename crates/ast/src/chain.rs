//! Long chains: dropping and cloning them without recursion.
//!
//! Generated PHP holds expressions that are hundreds of thousands of nodes
//! deep, all down one side:
//!
//! ```php
//! <?php
//! $total = 1 + 1 + 1 + /* ... 600,000 more ... */ 1;   // ((1 + 1) + 1) + ...
//! $config->set('a', 1)->set('b', 2)->set('c', 3);       // a method chain
//! $label = $first ?: $second ?: $third ?: 'none';        // (($first ?: $second) ?: ...)
//! ```
//!
//! Each step of such a chain keeps the rest of the chain in one child, its
//! *chain child*: the left operand of `+`, the object of `->`, the condition
//! of `?:`. The derived `Drop` and `Clone` would recurse once per step and
//! overflow the Rust stack, so [`Expression`] implements both by walking the
//! chain children in a loop. Every other child nests only as deep as the
//! parser's nesting limit allows, and is dropped and cloned the ordinary way.

use crate::{Expression, ExpressionKind};

impl Expression {
    /// The child a chain continues through, if this node is a chain step.
    ///
    /// For `$a + $b` it is `$a`, for `$a->b()` it is `$a`, for `-$a` it is
    /// `$a`. Code that walks a whole expression tree follows this child in a
    /// loop, so it never recurses along a chain.
    pub fn chain_child(&self) -> Option<&Expression> {
        match &self.kind {
            ExpressionKind::Binary { left: child, .. }
            | ExpressionKind::Index { target: child, .. }
            | ExpressionKind::Property { target: child, .. }
            | ExpressionKind::MethodCall { target: child, .. }
            | ExpressionKind::IncDec { target: child, .. }
            | ExpressionKind::Call { callee: child, .. }
            | ExpressionKind::StaticProperty { class: child, .. }
            | ExpressionKind::ClassConstant { class: child, .. }
            | ExpressionKind::StaticCall { class: child, .. }
            | ExpressionKind::InstanceOf { value: child, .. }
            | ExpressionKind::Ternary {
                condition: child, ..
            }
            | ExpressionKind::Unary { operand: child, .. }
            | ExpressionKind::Cast { operand: child, .. } => Some(child),
            _ => None,
        }
    }
}

impl ExpressionKind {
    fn chain_child_mut(&mut self) -> Option<&mut Box<Expression>> {
        match self {
            ExpressionKind::Binary { left: child, .. }
            | ExpressionKind::Index { target: child, .. }
            | ExpressionKind::Property { target: child, .. }
            | ExpressionKind::MethodCall { target: child, .. }
            | ExpressionKind::IncDec { target: child, .. }
            | ExpressionKind::Call { callee: child, .. }
            | ExpressionKind::StaticProperty { class: child, .. }
            | ExpressionKind::ClassConstant { class: child, .. }
            | ExpressionKind::StaticCall { class: child, .. }
            | ExpressionKind::InstanceOf { value: child, .. }
            | ExpressionKind::Ternary {
                condition: child, ..
            }
            | ExpressionKind::Unary { operand: child, .. }
            | ExpressionKind::Cast { operand: child, .. } => Some(child),
            _ => None,
        }
    }
}

impl Expression {
    /// A copy of this node alone: every field is cloned except the chain
    /// child, which is left as a placeholder for [`Clone::clone`] to fill in.
    fn clone_step(&self) -> Expression {
        let hole = || {
            Box::new(Expression {
                kind: ExpressionKind::EMPTY,
                span: self.span,
            })
        };
        let kind = match &self.kind {
            ExpressionKind::Binary {
                operator, right, ..
            } => ExpressionKind::Binary {
                operator: *operator,
                left: hole(),
                right: right.clone(),
            },
            ExpressionKind::Index { index, .. } => ExpressionKind::Index {
                target: hole(),
                index: index.clone(),
            },
            ExpressionKind::Property { name, nullsafe, .. } => ExpressionKind::Property {
                target: hole(),
                name: name.clone(),
                nullsafe: *nullsafe,
            },
            ExpressionKind::MethodCall {
                name,
                arguments,
                nullsafe,
                ..
            } => ExpressionKind::MethodCall {
                target: hole(),
                name: name.clone(),
                arguments: arguments.clone(),
                nullsafe: *nullsafe,
            },
            ExpressionKind::IncDec {
                increment, prefix, ..
            } => ExpressionKind::IncDec {
                target: hole(),
                increment: *increment,
                prefix: *prefix,
            },
            ExpressionKind::Call {
                arguments, opening, ..
            } => ExpressionKind::Call {
                opening: *opening,
                callee: hole(),
                arguments: arguments.clone(),
            },
            ExpressionKind::StaticProperty {
                name, name_start, ..
            } => ExpressionKind::StaticProperty {
                name_start: *name_start,
                class: hole(),
                name: name.clone(),
            },
            ExpressionKind::ClassConstant { name, .. } => ExpressionKind::ClassConstant {
                class: hole(),
                name: name.clone(),
            },
            ExpressionKind::StaticCall {
                name, arguments, ..
            } => ExpressionKind::StaticCall {
                class: hole(),
                name: name.clone(),
                arguments: arguments.clone(),
            },
            ExpressionKind::InstanceOf { class, .. } => ExpressionKind::InstanceOf {
                value: hole(),
                class: class.clone(),
            },
            ExpressionKind::Ternary {
                then, otherwise, ..
            } => ExpressionKind::Ternary {
                condition: hole(),
                then: then.clone(),
                otherwise: otherwise.clone(),
            },
            ExpressionKind::Unary { operator, .. } => ExpressionKind::Unary {
                operator: *operator,
                operand: hole(),
            },
            ExpressionKind::Cast { kind, .. } => ExpressionKind::Cast {
                kind: *kind,
                operand: hole(),
            },
            // Not a chain step: nothing below it follows a chain, so the
            // ordinary clone is shallow enough.
            other => other.clone(),
        };
        Expression {
            kind,
            span: self.span,
        }
    }
}

impl Clone for Expression {
    /// Copies the chain steps top-down into a list, then links the copies
    /// back together from the bottom up.
    fn clone(&self) -> Self {
        let mut steps = Vec::new();
        let mut node = self;
        while let Some(child) = node.chain_child() {
            steps.push(node.clone_step());
            node = child;
        }
        let mut copy = node.clone_step();
        while let Some(mut step) = steps.pop() {
            if let Some(hole) = step.kind.chain_child_mut() {
                **hole = copy;
            }
            copy = step;
        }
        copy
    }
}

impl Drop for Expression {
    /// Unlinks the chain children in a loop instead of by recursion: each
    /// step's contents are moved out before the step itself is dropped, so
    /// every step drops with an empty chain child.
    fn drop(&mut self) {
        let mut kind = std::mem::replace(&mut self.kind, ExpressionKind::EMPTY);
        while let Some(child) = kind.chain_child_mut() {
            let rest = std::mem::replace(&mut child.kind, ExpressionKind::EMPTY);
            kind = rest;
        }
    }
}
