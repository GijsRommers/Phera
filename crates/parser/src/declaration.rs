//! Declarations: attributes, functions, classes (and interfaces, traits,
//! enums), class members with property hooks, `use` imports and constants.

use phera_ast::{
    Attribute, ClassDeclaration, ClassKind, ClassMember, ClassMemberKind, ConstantDeclaration,
    Expression, FunctionDeclaration, HookBody, Identifier, Method, Modifiers, Name, Parameter,
    PropertyDeclaration, PropertyHook, Statement, StatementKind, TraitAdaptation, UseItem, UseKind,
    Visibility,
};
use phera_lexer::TokenKind;
use phera_source::Span;

use crate::error::ParseError;
use crate::parser::{Parser, is_word_token};

impl Parser<'_> {
    /// `#[A, B(...)]` groups, flattened into one list.
    pub(crate) fn parse_attributes(&mut self) -> Result<Vec<Attribute>, ParseError> {
        let mut attributes = Vec::new();

        while self.at(TokenKind::AttributeStart) {
            self.bump()?; // `#[`
            loop {
                let start = self.peek_span();
                let name = self.parse_class_name()?;
                let arguments = if self.at(TokenKind::LeftParen) {
                    match self.parse_argument_list()? {
                        phera_ast::Arguments::List(items) => items,
                        // `#[A(...)]` parses in PHP (and fails at compile time);
                        // the AST has no placeholder, so it becomes no arguments.
                        phera_ast::Arguments::FirstClassCallable => Vec::new(),
                    }
                } else {
                    Vec::new()
                };
                attributes.push(Attribute {
                    name,
                    arguments,
                    span: self.span_since(start),
                });
                if !self.eat(TokenKind::Comma)? {
                    break;
                }
            }
            self.expect(TokenKind::RightBracket, "']'")?;
        }

        Ok(attributes)
    }

    /// A declaration that may carry attributes: function, class, trait,
    /// interface, enum, and (at the top level) `const`.
    pub(crate) fn parse_attributed_declaration(
        &mut self,
        attributes: Vec<Attribute>,
        top: bool,
    ) -> Result<Statement, ParseError> {
        let start = self.peek_span();

        match self.kind() {
            TokenKind::Function => {
                let function = self.parse_function_declaration(attributes)?;
                Ok(Statement {
                    kind: StatementKind::Function(Box::new(function)),
                    span: self.span_since(start),
                })
            }
            // `readonly` is both a class modifier and a valid function name.
            TokenKind::Readonly if self.at2(TokenKind::Function) => {
                let function = self.parse_function_declaration(attributes)?;
                Ok(Statement {
                    kind: StatementKind::Function(Box::new(function)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Class
            | TokenKind::Trait
            | TokenKind::Interface
            | TokenKind::Enum
            | TokenKind::Abstract
            | TokenKind::Final
            | TokenKind::Readonly => {
                let class = self.parse_class_declaration(attributes)?;
                Ok(Statement {
                    kind: StatementKind::Class(Box::new(class)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Const if top => self.parse_const_statement(attributes),
            _ => Err(self.error(format!(
                "expected a declaration after the attributes, found {}",
                self.describe_current()
            ))),
        }
    }

    /// `function [&] name(params) [: type] { ... }`
    pub(crate) fn parse_function_declaration(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<FunctionDeclaration, ParseError> {
        self.expect(TokenKind::Function, "'function'")?;
        let by_reference = self.eat(TokenKind::Ampersand)?;
        let name = if self.at(TokenKind::Readonly) {
            let span = self.peek_span();
            self.bump()?;
            Identifier {
                name: "readonly".to_string(),
                span,
            }
        } else {
            self.parse_strict_identifier()?
        };
        let parameters = self.parse_parameter_list()?;
        let return_type = self.parse_return_type()?;
        self.expect(TokenKind::LeftBrace, "'{'")?;
        let body = self.parse_inner_statements()?;
        self.close_brace()?;

        Ok(FunctionDeclaration {
            attributes,
            name,
            by_reference,
            parameters,
            return_type,
            body,
        })
    }

    /// `class` / `trait` / `interface` / `enum` with modifiers, name, heritage
    /// and members.
    pub(crate) fn parse_class_declaration(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<ClassDeclaration, ParseError> {
        let start = self.peek_span();

        let mut modifiers = Modifiers::default();
        loop {
            if self.eat(TokenKind::Abstract)? {
                modifiers.is_abstract = true;
            } else if self.eat(TokenKind::Final)? {
                modifiers.is_final = true;
            } else if self.eat(TokenKind::Readonly)? {
                modifiers.is_readonly = true;
            } else {
                break;
            }
        }

        let kind = match self.kind() {
            TokenKind::Class => ClassKind::Class,
            TokenKind::Trait => ClassKind::Trait,
            TokenKind::Interface => ClassKind::Interface,
            TokenKind::Enum => ClassKind::Enum,
            _ => {
                return Err(self.error(format!(
                    "expected 'class', 'trait', 'interface' or 'enum', found {}",
                    self.describe_current()
                )));
            }
        };
        self.bump()?;

        let name = Some(self.parse_strict_identifier()?);

        let backing_type = if kind == ClassKind::Enum && self.eat(TokenKind::Colon)? {
            Some(self.parse_type_expr(true)?)
        } else {
            None
        };

        // Only classes and interfaces can extend; traits and enums cannot.
        let extends = if self.eat(TokenKind::Extends)? {
            if !matches!(kind, ClassKind::Class | ClassKind::Interface) {
                return Err(self.error_at(
                    self.peek_span(),
                    "syntax error: unexpected token \"extends\"",
                ));
            }
            let mut names = vec![self.parse_class_name()?];
            // Interfaces extend a list; classes take a single name.
            if kind == ClassKind::Interface {
                while self.eat(TokenKind::Comma)? {
                    names.push(self.parse_class_name()?);
                }
            }
            names
        } else {
            Vec::new()
        };

        let implements = if self.eat(TokenKind::Implements)? {
            let mut names = vec![self.parse_class_name()?];
            while self.eat(TokenKind::Comma)? {
                names.push(self.parse_class_name()?);
            }
            names
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LeftBrace, "'{'")?;
        let members = self.parse_class_members()?;
        self.close_brace()?;

        Ok(ClassDeclaration {
            attributes,
            kind,
            modifiers,
            name,
            extends,
            implements,
            backing_type,
            members,
            span: self.span_since(start),
        })
    }

    /// `new [attrs] [modifiers] class(...) extends ... implements ... { ... }`.
    /// Returns the anonymous class declaration; the constructor arguments are
    /// attached by the caller's `new`.
    pub(crate) fn parse_anonymous_class(
        &mut self,
        attributes: Vec<Attribute>,
        modifiers: Modifiers,
        arguments: &mut phera_ast::Arguments,
    ) -> Result<ClassDeclaration, ParseError> {
        let start = self.expect(TokenKind::Class, "'class'")?.span;
        *arguments = if self.at(TokenKind::LeftParen) {
            self.parse_argument_list()?
        } else {
            phera_ast::Arguments::List(vec![])
        };
        let extends = if self.eat(TokenKind::Extends)? {
            vec![self.parse_class_name()?]
        } else {
            Vec::new()
        };
        let implements = if self.eat(TokenKind::Implements)? {
            let mut names = vec![self.parse_class_name()?];
            while self.eat(TokenKind::Comma)? {
                names.push(self.parse_class_name()?);
            }
            names
        } else {
            Vec::new()
        };
        self.expect(TokenKind::LeftBrace, "'{'")?;
        let members = self.parse_class_members()?;
        self.close_brace()?;

        Ok(ClassDeclaration {
            attributes,
            kind: ClassKind::Class,
            modifiers,
            name: None,
            extends,
            implements,
            backing_type: None,
            members,
            span: self.span_since(start),
        })
    }

    fn parse_class_members(&mut self) -> Result<Vec<ClassMember>, ParseError> {
        let mut members = Vec::new();

        while self.kind() != TokenKind::RightBrace && self.kind() != TokenKind::Eof && !self.halted
        {
            if self.at(TokenKind::Semicolon) {
                self.bump()?;
                continue;
            }

            let start = self.peek_span();
            let attributes = if self.at(TokenKind::AttributeStart) {
                self.parse_attributes()?
            } else {
                Vec::new()
            };

            if self.at(TokenKind::Use) {
                let member = self.parse_trait_use()?;
                members.push(ClassMember {
                    kind: member,
                    span: self.span_since(start),
                });
                continue;
            }

            if self.at(TokenKind::Case) {
                let (name, value) = self.parse_enum_case()?;
                members.push(ClassMember {
                    kind: ClassMemberKind::EnumCase {
                        attributes,
                        name,
                        value,
                    },
                    span: self.span_since(start),
                });
                continue;
            }

            // Modifiers (`public`, `static`, ..., or `var`), in any order.
            let mut modifiers = Modifiers::default();
            let mut has_modifiers = false;
            if self.at(TokenKind::Var) {
                self.bump()?;
                modifiers.visibility = Some(Visibility::Public);
                has_modifiers = true;
            }
            loop {
                if let Some((visibility, is_set_only)) = member_modifier(self.kind()) {
                    self.bump()?;
                    has_modifiers = true;
                    if is_set_only {
                        modifiers.set_visibility = Some(visibility);
                    } else {
                        modifiers.visibility = Some(visibility);
                    }
                } else {
                    match self.kind() {
                        TokenKind::Static => {
                            self.bump()?;
                            modifiers.is_static = true;
                            has_modifiers = true;
                        }
                        TokenKind::Abstract => {
                            self.bump()?;
                            modifiers.is_abstract = true;
                            has_modifiers = true;
                        }
                        TokenKind::Final => {
                            self.bump()?;
                            modifiers.is_final = true;
                            has_modifiers = true;
                        }
                        TokenKind::Readonly => {
                            self.bump()?;
                            modifiers.is_readonly = true;
                            has_modifiers = true;
                        }
                        _ => break,
                    }
                }
            }

            if self.at(TokenKind::Const) {
                self.bump()?;
                // `const Foo = 1` names a constant; `const int Foo = 1` has a
                // type. A single word directly followed by `=` is the name.
                let ty = if self.can_start_type(true)
                    && !(is_word_token(self.kind()) && self.at2(TokenKind::Assign))
                {
                    Some(self.parse_type_expr(true)?)
                } else {
                    None
                };
                let items = self.parse_const_items(false)?;
                self.expect(TokenKind::Semicolon, "';'")?;
                members.push(ClassMember {
                    kind: ClassMemberKind::Constant {
                        attributes,
                        modifiers,
                        ty,
                        items,
                    },
                    span: self.span_since(start),
                });
                continue;
            }

            if self.at(TokenKind::Function) {
                let method = self.parse_method(attributes, modifiers)?;
                members.push(ClassMember {
                    kind: ClassMemberKind::Method(method),
                    span: self.span_since(start),
                });
                continue;
            }

            if self.at(TokenKind::Variable) || self.can_start_type(false) {
                if !has_modifiers {
                    return Err(self.error("properties must have a visibility modifier or 'var'"));
                }
                let ty = if self.can_start_type(false) {
                    Some(self.parse_type_expr(false)?)
                } else {
                    None
                };
                let items = self.parse_property_list()?;
                members.push(ClassMember {
                    kind: ClassMemberKind::Property {
                        attributes,
                        modifiers,
                        ty,
                        items,
                    },
                    span: self.span_since(start),
                });
                continue;
            }

            return Err(self.error(format!(
                "expected a class member, found {}",
                self.describe_current()
            )));
        }

        Ok(members)
    }

    /// `use A, B;` or `use A, B { ... adaptations ... }` inside a class.
    fn parse_trait_use(&mut self) -> Result<ClassMemberKind, ParseError> {
        self.expect(TokenKind::Use, "'use'")?;
        let mut traits = vec![self.parse_class_name()?];
        while self.eat(TokenKind::Comma)? {
            traits.push(self.parse_class_name()?);
        }

        let adaptations = if self.eat(TokenKind::Semicolon)? {
            Vec::new()
        } else {
            self.expect(TokenKind::LeftBrace, "'{' or ';'")?;
            let mut adaptations = Vec::new();
            while !self.at(TokenKind::RightBrace) && self.kind() != TokenKind::Eof {
                adaptations.push(self.parse_trait_adaptation()?);
            }
            self.expect(TokenKind::RightBrace, "'}'")?;
            adaptations
        };

        Ok(ClassMemberKind::TraitUse {
            traits,
            adaptations,
        })
    }

    fn parse_trait_adaptation(&mut self) -> Result<TraitAdaptation, ParseError> {
        // `method`, `Trait::method` (the trait may be a qualified name)
        let (trait_name, method) = {
            let first = if matches!(
                self.kind(),
                TokenKind::Identifier
                    | TokenKind::QualifiedName
                    | TokenKind::FullyQualifiedName
                    | TokenKind::RelativeName
            ) {
                self.parse_name()?
            } else {
                identifier_to_name(self.parse_identifier()?)
            };
            if self.at(TokenKind::DoubleColon) {
                self.bump()?;
                let method = self.parse_identifier()?;
                (Some(first), method)
            } else {
                let method = Identifier {
                    name: first.text,
                    span: first.span,
                };
                (None, method)
            }
        };

        if self.eat(TokenKind::InsteadOf)? {
            let mut instead_of = vec![self.parse_class_name()?];
            while self.eat(TokenKind::Comma)? {
                instead_of.push(self.parse_class_name()?);
            }
            self.expect(TokenKind::Semicolon, "';'")?;
            let trait_name = trait_name.ok_or_else(|| {
                self.error("'insteadof' requires a trait-qualified method reference")
            })?;
            Ok(TraitAdaptation::Precedence {
                trait_name,
                method,
                instead_of,
            })
        } else {
            self.expect(TokenKind::As, "'as' or 'insteadof'")?;
            // `as alias`, `as visibility`, `as visibility alias`
            let mut visibility = None;
            let mut alias = None;
            if let Some((modifier, _)) = member_modifier(self.kind()) {
                self.bump()?;
                visibility = Some(modifier);
                if is_word_token(self.kind()) && self.kind() != TokenKind::As {
                    alias = Some(self.parse_identifier()?);
                }
            } else if is_word_token(self.kind()) {
                alias = Some(self.parse_identifier()?);
            }
            self.expect(TokenKind::Semicolon, "';'")?;
            Ok(TraitAdaptation::Alias {
                trait_name,
                method,
                alias,
                visibility,
            })
        }
    }

    /// `case Name [= expr];` of an enum.
    fn parse_enum_case(&mut self) -> Result<(Identifier, Option<Expression>), ParseError> {
        self.expect(TokenKind::Case, "'case'")?;
        let name = self.parse_identifier()?;
        let value = if self.eat(TokenKind::Assign)? {
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        self.expect(TokenKind::Semicolon, "';'")?;
        Ok((name, value))
    }

    /// `function [&] name(params) [: type] (; | { ... })`
    fn parse_method(
        &mut self,
        attributes: Vec<Attribute>,
        modifiers: Modifiers,
    ) -> Result<Method, ParseError> {
        self.expect(TokenKind::Function, "'function'")?;
        let by_reference = self.eat(TokenKind::Ampersand)?;
        let name = self.parse_identifier()?;
        let parameters = self.parse_parameter_list()?;
        let return_type = self.parse_return_type()?;

        let body = if self.eat(TokenKind::Semicolon)? {
            None
        } else {
            self.expect(TokenKind::LeftBrace, "'{' or ';'")?;
            let statements = self.parse_inner_statements()?;
            self.close_brace()?;
            Some(statements)
        };

        Ok(Method {
            attributes,
            modifiers,
            name,
            by_reference,
            parameters,
            return_type,
            body,
        })
    }

    /// `$a [= expr] [, $b ...] ;` or a single hooked property `$a [= expr] { hooks }`.
    fn parse_property_list(&mut self) -> Result<Vec<PropertyDeclaration>, ParseError> {
        let mut items = Vec::new();

        loop {
            let start = self.peek_span();
            let token = self.expect(TokenKind::Variable, "a property")?;
            let name = self.text_of(&token)[1..].to_string();
            let default = if self.eat(TokenKind::Assign)? {
                Some(self.parse_expr(0)?)
            } else {
                None
            };

            let hooks = if self.at(TokenKind::LeftBrace) {
                self.bump()?;
                let hooks = self.parse_property_hooks()?;
                self.expect(TokenKind::RightBrace, "'}'")?;
                hooks
            } else {
                Vec::new()
            };

            let has_hooks = !hooks.is_empty();
            items.push(PropertyDeclaration {
                name,
                default,
                hooks,
                span: self.span_since(start),
            });

            if !has_hooks && self.eat(TokenKind::Comma)? {
                continue;
            }
            break;
        }

        if self.at(TokenKind::Semicolon) {
            self.bump()?;
        }

        Ok(items)
    }

    /// Property hooks: `get => expr;`, `set($v) { ... }`, `get;`.
    fn parse_property_hooks(&mut self) -> Result<Vec<PropertyHook>, ParseError> {
        let mut hooks = Vec::new();

        while !self.at(TokenKind::RightBrace) && self.kind() != TokenKind::Eof {
            let start = self.peek_span();
            let attributes = if self.at(TokenKind::AttributeStart) {
                self.parse_attributes()?
            } else {
                Vec::new()
            };

            // Modifiers on hooks: only `final` is meaningful; PHP rejects the
            // others at compile time, so they are dropped here.
            let mut is_final = false;
            loop {
                if self.eat(TokenKind::Final)? {
                    is_final = true;
                } else if matches!(
                    self.kind(),
                    TokenKind::Public
                        | TokenKind::Protected
                        | TokenKind::Private
                        | TokenKind::PublicSet
                        | TokenKind::ProtectedSet
                        | TokenKind::PrivateSet
                        | TokenKind::Static
                        | TokenKind::Abstract
                        | TokenKind::Readonly
                ) {
                    self.bump()?;
                } else {
                    break;
                }
            }

            let by_reference = self.eat(TokenKind::Ampersand)?;
            let name = self.parse_strict_identifier()?;
            let parameters = if self.at(TokenKind::LeftParen) {
                self.parse_parameter_list()?
            } else {
                Vec::new()
            };

            let body = if self.eat(TokenKind::Semicolon)? {
                HookBody::None
            } else if self.at(TokenKind::LeftBrace) {
                self.bump()?;
                let statements = self.parse_inner_statements()?;
                self.close_brace()?;
                HookBody::Block(statements)
            } else {
                self.expect(TokenKind::FatArrow, "'=>'")?;
                let expression = self.parse_expr(0)?;
                self.expect(TokenKind::Semicolon, "';'")?;
                HookBody::Expression(expression)
            };

            hooks.push(PropertyHook {
                attributes,
                is_final,
                by_reference,
                name,
                parameters,
                body,
                span: self.span_since(start),
            });
        }

        Ok(hooks)
    }

    /// `(...)` parameter list.
    pub(crate) fn parse_parameter_list(&mut self) -> Result<Vec<Parameter>, ParseError> {
        self.expect(TokenKind::LeftParen, "'('")?;
        let mut parameters = Vec::new();

        if !self.at(TokenKind::RightParen) {
            loop {
                parameters.push(self.parse_parameter()?);
                if !self.eat(TokenKind::Comma)? {
                    break;
                }
                if self.at(TokenKind::RightParen) {
                    break;
                }
            }
        }

        self.expect(TokenKind::RightParen, "')'")?;
        Ok(parameters)
    }

    /// One parameter: attributes, promotion modifiers, type, `&`, `...`,
    /// `$name`, default, and hooks on promoted properties.
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let start = self.peek_span();
        let attributes = if self.at(TokenKind::AttributeStart) {
            self.parse_attributes()?
        } else {
            Vec::new()
        };

        let mut modifiers = Modifiers::default();
        loop {
            match self.kind() {
                TokenKind::Public | TokenKind::Protected | TokenKind::Private => {
                    modifiers.visibility = Some(match self.kind() {
                        TokenKind::Public => Visibility::Public,
                        TokenKind::Protected => Visibility::Protected,
                        _ => Visibility::Private,
                    });
                    self.bump()?;
                }
                TokenKind::PublicSet | TokenKind::ProtectedSet | TokenKind::PrivateSet => {
                    modifiers.set_visibility = Some(match self.kind() {
                        TokenKind::PublicSet => Visibility::Public,
                        TokenKind::ProtectedSet => Visibility::Protected,
                        _ => Visibility::Private,
                    });
                    self.bump()?;
                }
                TokenKind::Static => {
                    self.bump()?;
                    modifiers.is_static = true;
                }
                TokenKind::Readonly => {
                    self.bump()?;
                    modifiers.is_readonly = true;
                }
                TokenKind::Abstract => {
                    self.bump()?;
                    modifiers.is_abstract = true;
                }
                TokenKind::Final => {
                    self.bump()?;
                    modifiers.is_final = true;
                }
                _ => break,
            }
        }

        let ty = if self.can_start_type(false) {
            Some(self.parse_type_expr(false)?)
        } else {
            None
        };

        let by_reference = self.eat(TokenKind::Ampersand)?;
        let variadic = self.eat(TokenKind::Ellipsis)?;
        let token = self.expect(TokenKind::Variable, "a parameter")?;
        let name = self.text_of(&token)[1..].to_string();
        let default = if self.eat(TokenKind::Assign)? {
            Some(self.parse_expr(0)?)
        } else {
            None
        };
        let hooks = if self.at(TokenKind::LeftBrace) {
            self.bump()?;
            let hooks = self.parse_property_hooks()?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            hooks
        } else {
            Vec::new()
        };

        Ok(Parameter {
            attributes,
            modifiers,
            ty,
            by_reference,
            variadic,
            name,
            default,
            hooks,
            span: self.span_since(start),
        })
    }

    /// `const A = 1, B = 2;` at the top level.
    pub(crate) fn parse_const_statement(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Const, "'const'")?.span;
        let items = self.parse_const_items(true)?;
        self.expect_end()?;

        Ok(Statement {
            kind: StatementKind::Const { attributes, items },
            span: self.span_since(start),
        })
    }

    /// `A = expr, B = expr`. Top-level constant names are plain identifiers;
    /// class constants also allow semi-reserved words.
    fn parse_const_items(&mut self, strict: bool) -> Result<Vec<ConstantDeclaration>, ParseError> {
        let mut items = Vec::new();

        loop {
            let start = self.peek_span();
            let name = if strict {
                self.parse_strict_identifier()?
            } else if is_word_token(self.kind()) {
                self.parse_identifier()?
            } else {
                return Err(self.error(format!(
                    "expected a constant name, found {}",
                    self.describe_current()
                )));
            };
            self.expect(TokenKind::Assign, "'='")?;
            let value = self.parse_expr(0)?;
            items.push(ConstantDeclaration {
                name,
                value,
                span: self.span_since(start),
            });
            if !self.eat(TokenKind::Comma)? {
                break;
            }
        }

        Ok(items)
    }

    /// `use ...;` at the top level, with group uses flattened.
    pub(crate) fn parse_use_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Use, "'use'")?.span;

        let outer_kind = if self.at(TokenKind::Function) {
            self.bump()?;
            Some(UseKind::Function)
        } else if self.at(TokenKind::Const) {
            self.bump()?;
            Some(UseKind::Constant)
        } else {
            None
        };

        let first = self.parse_legacy_name()?;

        let mut items = Vec::new();

        if self.at(TokenKind::Backslash) && self.at2(TokenKind::LeftBrace) {
            // Group use: `use Foo\{Bar, function baz as b, const X};`
            self.bump()?; // `\`
            self.expect(TokenKind::LeftBrace, "'{'")?;

            if self.at(TokenKind::RightBrace) {
                return Err(self.error("a group use must list at least one name"));
            }

            {
                loop {
                    let item_start = self.peek_span();
                    let kind = match outer_kind {
                        Some(kind) => kind,
                        None => {
                            if self.at(TokenKind::Function) {
                                self.bump()?;
                                UseKind::Function
                            } else if self.at(TokenKind::Const) {
                                self.bump()?;
                                UseKind::Constant
                            } else {
                                UseKind::Class
                            }
                        }
                    };
                    let name = self.parse_use_name()?;
                    let alias = self.parse_use_alias()?;
                    items.push(UseItem {
                        kind,
                        name: join_name(&first, &name),
                        alias,
                        span: self.span_since(item_start),
                    });
                    if !self.eat(TokenKind::Comma)? {
                        break;
                    }
                    if self.at(TokenKind::RightBrace) {
                        break;
                    }
                }
            }

            self.expect(TokenKind::RightBrace, "'}'")?;
            self.expect_end()?;
        } else {
            let kind = outer_kind.unwrap_or(UseKind::Class);
            let alias = self.parse_use_alias()?;
            items.push(UseItem {
                kind,
                name: first.clone(),
                alias,
                span: self.span_since(start),
            });

            while self.eat(TokenKind::Comma)? {
                let item_start = self.peek_span();
                let name = self.parse_legacy_name()?;
                let alias = self.parse_use_alias()?;
                items.push(UseItem {
                    kind,
                    name,
                    alias,
                    span: self.span_since(item_start),
                });
            }

            self.expect_end()?;
        }

        Ok(Statement {
            kind: StatementKind::Use(items),
            span: self.span_since(start),
        })
    }

    /// A use-declaration name, with a leading `\` allowed.
    fn parse_legacy_name(&mut self) -> Result<Name, ParseError> {
        match self.kind() {
            TokenKind::FullyQualifiedName | TokenKind::Identifier | TokenKind::QualifiedName => {
                self.parse_name()
            }
            _ => Err(self.error(format!(
                "expected a name in 'use', found {}",
                self.describe_current()
            ))),
        }
    }

    /// A name inside a group use: no leading `\`.
    fn parse_use_name(&mut self) -> Result<Name, ParseError> {
        match self.kind() {
            TokenKind::Identifier | TokenKind::QualifiedName => self.parse_name(),
            _ => Err(self.error(format!(
                "expected a name in 'use', found {}",
                self.describe_current()
            ))),
        }
    }

    /// `as Alias`, where the alias is a plain identifier.
    fn parse_use_alias(&mut self) -> Result<Option<Identifier>, ParseError> {
        if self.eat(TokenKind::As)? {
            Ok(Some(self.parse_strict_identifier()?))
        } else {
            Ok(None)
        }
    }
}

/// A visibility modifier token: `public`, `protected`, `private` and the
/// asymmetric `public(set)` etc. Returns the visibility and whether it is a
/// set-only modifier.
fn member_modifier(kind: TokenKind) -> Option<(Visibility, bool)> {
    let visibility = match kind {
        TokenKind::Public => Visibility::Public,
        TokenKind::Protected => Visibility::Protected,
        TokenKind::Private => Visibility::Private,
        TokenKind::PublicSet => return Some((Visibility::Public, true)),
        TokenKind::ProtectedSet => return Some((Visibility::Protected, true)),
        TokenKind::PrivateSet => return Some((Visibility::Private, true)),
        _ => return None,
    };
    Some((visibility, false))
}

fn identifier_to_name(identifier: Identifier) -> Name {
    Name {
        kind: phera_ast::NameKind::Unqualified,
        text: identifier.name,
        span: identifier.span,
    }
}

/// Joins a group-use prefix with an item name: `Foo\{Bar}` is `Foo\Bar`.
fn join_name(prefix: &Name, item: &Name) -> Name {
    let kind = if prefix.kind == phera_ast::NameKind::FullyQualified {
        phera_ast::NameKind::FullyQualified
    } else {
        phera_ast::NameKind::Qualified
    };
    Name {
        kind,
        text: format!("{}\\{}", prefix.text, item.text),
        span: Span::new(prefix.span.start, item.span.end),
    }
}
