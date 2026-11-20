use phera_source::Span;

/// A single name that can never be namespaced: a method, property, constant,
/// label, named argument, or the name being declared.
///
/// Reserved words are allowed in most of these positions (`$a->list`,
/// `Foo::class`), so the text is kept exactly as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameKind {
    /// `Foo`
    Unqualified,
    /// `Foo\Bar`
    Qualified,
    /// `\Foo\Bar`
    FullyQualified,
    /// `namespace\Foo`
    Relative,
}

/// A class, function or constant name as written, before resolution.
///
/// `text` never has the leading `\` of a fully qualified name or the
/// `namespace\` prefix of a relative one; `kind` records which form it was.
/// Keywords used as names (`static`, `array`, `callable`, `self`, `parent`)
/// arrive here too, as unqualified names.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    pub kind: NameKind,
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// A class name or a built-in type such as `int`, `array`, `static`.
    Named(Name),
    /// `?T`
    Nullable(Box<Type>),
    /// `A|B`
    Union(Vec<Type>),
    /// `A&B`
    Intersection(Vec<Type>),
}
