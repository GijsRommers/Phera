use phera_source::Span;

use crate::{
    Attribute, ClassDeclaration, Expression, FunctionDeclaration, Identifier, Name, Variable,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

pub type Block = Vec<Statement>;

#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    Expression(Expression),
    /// `echo $a, $b;` and `<?= $a ?>`
    Echo(Vec<Expression>),
    /// Text outside `<?php ... ?>`, printed as is.
    InlineHtml(Vec<u8>),
    Block(Block),

    /// A whole `if` ladder:
    ///
    /// ```php
    /// if ($a) { A(); } elseif ($b) { B(); } elseif ($c) { C(); } else { D(); }
    /// ```
    ///
    /// `then` is `A();`, `elseifs` holds the `$b` and `$c` branches in source
    /// order, `otherwise` is `D();`. Generated code has ladders of a hundred
    /// thousand `elseif`s; as a list they are one node, not that many nested
    /// ones that every pass would recurse through.
    ///
    /// `else if` (two words) is PHP's `else` followed by an ordinary `if`
    /// statement, and stays nested, as in PHP's grammar: `otherwise` holds
    /// that one `If`. The `if: ... endif;` form parses to the same node.
    If {
        condition: Expression,
        then: Block,
        elseifs: Vec<ElseIf>,
        otherwise: Option<Block>,
    },
    While {
        condition: Expression,
        body: Block,
    },
    DoWhile {
        body: Block,
        condition: Expression,
    },
    /// Each part is a comma-separated list; the last condition decides.
    For {
        init: Vec<Expression>,
        condition: Vec<Expression>,
        step: Vec<Expression>,
        body: Block,
    },
    /// `value` is a [`crate::ExpressionKind::List`] when destructuring.
    Foreach {
        subject: Expression,
        key: Option<Expression>,
        value: Expression,
        by_reference: bool,
        body: Block,
    },
    Switch {
        subject: Expression,
        cases: Vec<SwitchCase>,
    },
    /// `break;` is `Break(1)`.
    Break(u32),
    Continue(u32),
    Return(Option<Expression>),

    Global(Vec<Variable>),
    /// `static $a = 1, $b;`
    Static(Vec<StaticVariable>),
    Unset(Vec<Expression>),
    Try {
        body: Block,
        catches: Vec<Catch>,
        finally: Option<Block>,
    },
    /// `declare(strict_types=1);` has no body.
    Declare {
        directives: Vec<DeclareDirective>,
        body: Option<Block>,
    },
    /// A file- or function-local jump to a case-sensitive label.
    Goto(Identifier),
    /// A standalone statement, even immediately before a closing brace.
    Label(Identifier),

    /// For the `namespace Foo;` form, `statements` holds everything up to the
    /// next namespace statement or the end of the file. The global namespace
    /// block `namespace { ... }` has no name.
    Namespace {
        name: Option<Name>,
        statements: Block,
        braced: bool,
    },
    /// Group uses are flattened: `use A\{B, C as D};` is two items.
    Use(Vec<UseItem>),
    Const {
        doc_comment: Option<Span>,
        attributes: Vec<Attribute>,
        items: Vec<ConstantDeclaration>,
    },
    Function(Box<FunctionDeclaration>),
    /// A class, interface, trait or enum.
    Class(Box<ClassDeclaration>),
    /// `__halt_compiler();` `data_offset` is the byte offset of the raw data
    /// after it.
    HaltCompiler {
        data_offset: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    /// `None` for `default:`.
    pub condition: Option<Expression>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticVariable {
    /// Without the `$`.
    pub name: String,
    pub default: Option<Expression>,
    pub span: Span,
}

/// One `elseif ($condition) { ... }` branch of [`StatementKind::If`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElseIf {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Catch {
    /// `catch (A | B $e)` has two types.
    pub types: Vec<Name>,
    /// Without the `$`. `None` for `catch (A)`.
    pub variable: Option<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeclareDirective {
    pub name: Identifier,
    pub value: Expression,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UseKind {
    /// `use Foo\Bar;` imports a class, interface, trait, enum or namespace.
    Class,
    Function,
    Constant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    pub kind: UseKind,
    /// The full imported name, with any group prefix already joined on.
    pub name: Name,
    pub alias: Option<Identifier>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstantDeclaration {
    pub name: Identifier,
    pub value: Expression,
    pub span: Span,
}
