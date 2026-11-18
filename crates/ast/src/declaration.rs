use phera_source::Span;

use crate::{Argument, ConstantDeclaration, Expression, Identifier, Name, Statement, Type};

/// `#[Name(args)]`. Attribute groups are flattened: `#[A, B]` is two entries.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub preceding_doc_comment: Option<Span>,
    pub name: Name,
    pub arguments: Vec<Argument>,
    pub first_class_callable: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDeclaration {
    pub span: Span,
    pub doc_comment: Option<Span>,
    pub attributes: Vec<Attribute>,
    pub name: Identifier,
    /// `function &foo()`
    pub by_reference: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub attributes: Vec<Attribute>,
    /// Non-empty only for constructor property promotion.
    pub modifiers: Modifiers,
    pub ty: Option<Type>,
    pub by_reference: bool,
    pub variadic: bool,
    /// Without the `$`.
    pub name: String,
    pub default: Option<Expression>,
    /// Hooks on a promoted property.
    pub hooks: Vec<PropertyHook>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub visibility: Option<Visibility>,
    /// `private(set)` and friends.
    pub set_visibility: Option<Visibility>,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly: bool,
}

impl Modifiers {
    pub fn promotes_property(&self) -> bool {
        self.visibility.is_some()
            || self.set_visibility.is_some()
            || self.is_readonly
            || self.is_final
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Trait,
    Enum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDeclaration {
    pub doc_comment: Option<Span>,
    pub attributes: Vec<Attribute>,
    pub kind: ClassKind,
    pub modifiers: Modifiers,
    /// `None` for an anonymous class.
    pub name: Option<Identifier>,
    /// One parent for a class; any number for an interface.
    pub extends: Vec<Name>,
    pub implements: Vec<Name>,
    /// `enum Suit: string`
    pub backing_type: Option<Type>,
    pub members: Vec<ClassMember>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassMember {
    pub doc_comment: Option<Span>,
    pub kind: ClassMemberKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClassMemberKind {
    /// `const A = 1, B = 2;`
    Constant {
        attributes: Vec<Attribute>,
        modifiers: Modifiers,
        ty: Option<Type>,
        items: Vec<ConstantDeclaration>,
    },
    /// `public int $a = 1, $b;`
    Property {
        attributes: Vec<Attribute>,
        modifiers: Modifiers,
        ty: Option<Type>,
        items: Vec<PropertyDeclaration>,
    },
    Method(Method),
    /// `use A, B { A::foo insteadof B; }`
    TraitUse {
        traits: Vec<Name>,
        adaptations: Vec<TraitAdaptation>,
    },
    /// `case Hearts = 'H';`
    EnumCase {
        attributes: Vec<Attribute>,
        name: Identifier,
        value: Option<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDeclaration {
    /// Without the `$`.
    pub name: String,
    pub default: Option<Expression>,
    pub hooks: Vec<PropertyHook>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub span: Span,
    pub attributes: Vec<Attribute>,
    pub modifiers: Modifiers,
    pub name: Identifier,
    pub by_reference: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    /// `None` for an abstract or interface method.
    pub body: Option<Vec<Statement>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitAdaptation {
    /// `A::foo insteadof B, C;`
    Precedence {
        trait_name: Name,
        method: Identifier,
        instead_of: Vec<Name>,
    },
    /// `foo as protected bar;`, `A::foo as bar;`, `foo as private;`
    Alias {
        trait_name: Option<Name>,
        method: Identifier,
        alias: Option<Identifier>,
        visibility: Option<Visibility>,
    },
}

/// `get => expr;`, `set(string $value) { ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyHook {
    pub attributes: Vec<Attribute>,
    pub is_final: bool,
    pub by_reference: bool,
    /// `get` or `set`.
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: HookBody,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HookBody {
    /// `get;` in an interface or abstract property.
    None,
    /// `get => expr;`
    Expression(Expression),
    Block(Vec<Statement>),
}
