use phera_source::Span;

use crate::{Attribute, ClassDeclaration, Identifier, Name, Parameter, Statement, Type};

#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

impl Expression {
    /// Takes the node apart. `Expression` implements `Drop`, so its fields
    /// cannot be moved out of it directly.
    pub fn into_kind(mut self) -> ExpressionKind {
        std::mem::replace(&mut self.kind, ExpressionKind::EMPTY)
    }
}

impl Drop for Expression {
    /// Unlinks left-deep chains in a loop instead of by recursion.
    ///
    /// The parser builds `a . b . c ...` and `$a->b()->c()...` in loops, so
    /// such chains can be tens of thousands of nodes deep in generated code.
    /// The default recursive drop would overflow the stack on them. Everything
    /// else nests only as deep as the parser's recursion limit allows, and is
    /// dropped the ordinary way.
    fn drop(&mut self) {
        let mut kind = std::mem::replace(&mut self.kind, ExpressionKind::EMPTY);

        loop {
            let next = match kind {
                ExpressionKind::Binary { left, .. } => left,
                ExpressionKind::Index { target, .. }
                | ExpressionKind::Property { target, .. }
                | ExpressionKind::MethodCall { target, .. }
                | ExpressionKind::IncDec { target, .. } => target,
                ExpressionKind::Call { callee, .. } => callee,
                ExpressionKind::StaticProperty { class, .. }
                | ExpressionKind::ClassConstant { class, .. }
                | ExpressionKind::StaticCall { class, .. } => class,
                ExpressionKind::InstanceOf { value, .. } => value,
                _ => return,
            };

            // The other fields of `kind` were dropped by the match. Take the
            // child's contents, so the child itself drops as an empty node.
            let mut next = *next;
            kind = std::mem::replace(&mut next.kind, ExpressionKind::EMPTY);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionKind {
    Literal(Literal),
    /// A double-quoted string or heredoc with at least one interpolation.
    InterpolatedString(Vec<StringPart>),
    Variable(Variable),
    /// A bare name: a constant (`PHP_EOL`), or the function or class named in
    /// a call, `new`, `instanceof` or `Foo::` position.
    Name(Name),
    MagicConstant(MagicConstant),

    /// `[1, 'a' => 2, ...$rest]` or `array(...)`.
    Array(Vec<ArrayItem>),
    /// A destructuring target: `[$a, , $b]` or `list('k' => $v)`. `None` is a
    /// skipped position.
    List(Vec<Option<ArrayItem>>),

    /// `$a[1]`, or `$a[]` when `index` is `None`.
    Index {
        target: Box<Expression>,
        index: Option<Box<Expression>>,
    },
    /// `$a->b` or `$a?->b`
    Property {
        target: Box<Expression>,
        name: MemberName,
        nullsafe: bool,
    },
    /// `Foo::$bar`. `class` is a [`ExpressionKind::Name`] unless the class is
    /// computed, as in `$class::$bar`.
    StaticProperty {
        class: Box<Expression>,
        name: Variable,
    },
    /// `Foo::BAR`, including `Foo::class`.
    ClassConstant {
        class: Box<Expression>,
        name: MemberName,
    },

    /// `foo(1)`, `$f(1)`, `(fn() => 1)()`. `callee` is a
    /// [`ExpressionKind::Name`] for a plain function call.
    Call {
        callee: Box<Expression>,
        arguments: Arguments,
    },
    /// `$a->b(1)` or `$a?->b(1)`
    MethodCall {
        target: Box<Expression>,
        name: MemberName,
        arguments: Arguments,
        nullsafe: bool,
    },
    /// `Foo::bar(1)`
    StaticCall {
        class: Box<Expression>,
        name: MemberName,
        arguments: Arguments,
    },
    /// `new Foo(1)`. Without parentheses the arguments are an empty list.
    New {
        class: NewTarget,
        arguments: Arguments,
    },

    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    /// `$a = 1`. `target` is a [`ExpressionKind::List`] when destructuring.
    Assign {
        target: Box<Expression>,
        value: Box<Expression>,
    },
    /// `$a = &$b`
    AssignReference {
        target: Box<Expression>,
        value: Box<Expression>,
    },
    /// `$a += 1`, `$a ??= 1`
    CompoundAssign {
        operator: BinaryOperator,
        target: Box<Expression>,
        value: Box<Expression>,
    },
    /// `++$a`, `$a--`
    IncDec {
        target: Box<Expression>,
        increment: bool,
        prefix: bool,
    },
    /// `$a ? $b : $c`, or `$a ?: $c` when `then` is `None`.
    Ternary {
        condition: Box<Expression>,
        then: Option<Box<Expression>>,
        otherwise: Box<Expression>,
    },
    Cast {
        kind: CastKind,
        operand: Box<Expression>,
    },
    InstanceOf {
        value: Box<Expression>,
        class: Box<Expression>,
    },

    Isset(Vec<Expression>),
    Empty(Box<Expression>),
    Include {
        kind: IncludeKind,
        path: Box<Expression>,
    },
    Eval(Box<Expression>),
    /// `exit`, `exit(1)`, `die('x')`
    Exit(Option<Box<Expression>>),
    Print(Box<Expression>),
    /// `clone $a`, or `clone($a, ['prop' => 1])` with `with` set.
    Clone {
        object: Box<Expression>,
        with: Option<Box<Expression>>,
    },
    Throw(Box<Expression>),
    /// `yield`, `yield $v`, `yield $k => $v`
    Yield {
        key: Option<Box<Expression>>,
        value: Option<Box<Expression>>,
    },
    YieldFrom(Box<Expression>),

    Closure(Box<Closure>),
    ArrowFunction(Box<ArrowFunction>),
    Match(Box<Match>),
}

impl ExpressionKind {
    /// What a node is left holding once its contents have been moved out.
    const EMPTY: ExpressionKind = ExpressionKind::Literal(Literal::Null);
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Int(i64),
    /// Also an integer literal too large for `i64`, as in PHP.
    Float(f64),
    /// The string's bytes with escape sequences already resolved. PHP strings
    /// are byte strings: `"\xff"` is one byte that is not valid UTF-8.
    String(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    /// Literal bytes, escape sequences resolved.
    Literal(Vec<u8>, Span),
    Expression(Expression),
    DollarCurly(Expression, DollarCurlyForm),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DollarCurlyForm {
    Name,
    Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variable {
    /// `$name`, stored without the `$`. `$this` is `Named("this")`.
    Named(String),
    /// `$$name` or `${expr}`: the expression gives the variable's name.
    Dynamic(Box<Expression>),
}

/// The member after `->` or `::`.
#[derive(Debug, Clone, PartialEq)]
pub enum MemberName {
    /// `$a->b`
    Identifier(Identifier),
    /// `$a->$b` (a variable expression) or `$a->{$b . 'c'}`
    Expression(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arguments {
    List(Vec<Argument>),
    /// `foo(...)`: creates a closure from the callee instead of calling it.
    FirstClassCallable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// Set for a named argument: `foo(name: 1)`.
    pub name: Option<Identifier>,
    pub value: Expression,
    /// `foo(...$args)`
    pub spread: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayItem {
    pub key: Option<Expression>,
    pub value: Expression,
    /// `[&$a]`
    pub by_reference: bool,
    /// `[...$a]`
    pub spread: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NewTarget {
    /// A [`ExpressionKind::Name`], or any expression for `new $class` and
    /// `new (expr)`.
    Class(Box<Expression>),
    /// `new class(...) { ... }`; the declaration has no name.
    Anonymous(Box<ClassDeclaration>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Plus,
    Minus,
    /// `!`
    Not,
    /// `~`
    BitwiseNot,
    /// `@`
    ErrorSuppress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    Concat,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    /// `&&` or `and`
    And,
    /// `||` or `or`
    Or,
    /// `xor`
    Xor,
    Equal,
    NotEqual,
    Identical,
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Spaceship,
    /// `??`
    NullCoalesce,
    /// `|>`
    Pipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Int,
    Float,
    String,
    Array,
    Object,
    Bool,
    Unset,
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeKind {
    Include,
    IncludeOnce,
    Require,
    RequireOnce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicConstant {
    Line,
    File,
    Dir,
    Class,
    Function,
    Method,
    Namespace,
    Trait,
    Property,
}

/// `function (...) use (...) { ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct Closure {
    pub declaration_span: Span,
    pub doc_comment: Option<Span>,
    pub attributes: Vec<Attribute>,
    pub is_static: bool,
    pub by_reference: bool,
    pub parameters: Vec<Parameter>,
    pub uses: Vec<ClosureUse>,
    pub return_type: Option<Type>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosureUse {
    /// Without the `$`.
    pub name: String,
    pub by_reference: bool,
    pub span: Span,
}

/// `fn (...) => expr`
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowFunction {
    pub declaration_span: Span,
    pub doc_comment: Option<Span>,
    pub attributes: Vec<Attribute>,
    pub is_static: bool,
    pub by_reference: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Expression,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub subject: Expression,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// `None` for the `default` arm.
    pub conditions: Option<Vec<Expression>>,
    pub body: Expression,
    pub span: Span,
}
