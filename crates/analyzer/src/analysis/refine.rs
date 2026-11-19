//! Guard facts: `isset($x) && print($x)` reads a defined variable on the right.
//! A work list handles arbitrarily long boolean chains without Rust recursion.

use super::*;

impl Analyzer<'_> {
    pub fn refine(&mut self, expression: &Expression, scope: &mut Scope, truth: bool) {
        let mut pending = vec![(expression, truth)];
        while let Some((expression, truth)) = pending.pop() {
            match &expression.kind {
                ExpressionKind::Variable(Variable::Named(name)) if truth => {
                    let ty = scope.get(name).ty;
                    scope.define(name, ty);
                }
                ExpressionKind::Isset(items) if truth => {
                    for item in items {
                        self.refine_defined(item, scope);
                    }
                }
                ExpressionKind::Empty(item) if !truth => self.refine_defined(item, scope),
                ExpressionKind::Unary {
                    operator: UnaryOperator::Not,
                    operand,
                } => pending.push((operand, !truth)),
                ExpressionKind::Binary {
                    operator: BinaryOperator::And,
                    left,
                    right,
                } if truth => {
                    pending.push((left, true));
                    pending.push((right, true));
                }
                ExpressionKind::Binary {
                    operator: BinaryOperator::Or,
                    left,
                    right,
                } if !truth => {
                    pending.push((left, false));
                    pending.push((right, false));
                }
                ExpressionKind::Call {
                    callee,
                    arguments: Arguments::List(arguments),
                } => self.refine_predicate(callee, arguments, scope, truth),
                ExpressionKind::InstanceOf { value, class } if truth => {
                    if let ExpressionKind::Variable(Variable::Named(name)) = &value.kind
                        && let ExpressionKind::Name(class) = &class.kind
                        && let Some(class) = self.class_name(class, false)
                    {
                        scope.define(name, ValueType::Class(class));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn refine_defined(&self, expression: &Expression, scope: &mut Scope) {
        let mut root = expression;
        while let ExpressionKind::Index { target, .. } | ExpressionKind::Property { target, .. } =
            &root.kind
        {
            root = target;
        }
        if let ExpressionKind::Variable(Variable::Named(name)) = &root.kind {
            let ty = scope.get(name).ty.without_null();
            scope.define(name, ty);
        }
    }

    fn refine_predicate(
        &mut self,
        callee: &Expression,
        arguments: &[Argument],
        scope: &mut Scope,
        truth: bool,
    ) {
        let (ExpressionKind::Name(name), Some(argument)) = (&callee.kind, arguments.first()) else {
            return;
        };
        let Some(function) = self.existence_function(name) else {
            return;
        };
        if function == "is_null" && !truth {
            if let ExpressionKind::Assign { target, .. } = &argument.value.kind {
                self.refine_defined(target, scope);
            } else if matches!(
                argument.value.kind,
                ExpressionKind::Variable(Variable::Named(_))
            ) {
                self.refine_defined(&argument.value, scope);
            }
        }
        if !truth {
            return;
        }
        let symbol = match function {
            "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                self.class_probe(&argument.value)
            }
            "property_exists" | "method_exists" => {
                let member = arguments
                    .get(1)
                    .and_then(|argument| literal_name(&argument.value));
                match (&argument.value.kind, member) {
                    (ExpressionKind::Variable(Variable::Named(receiver)), Some(member)) => {
                        Some(if function == "property_exists" {
                            Symbol::Property(receiver.clone(), member.into())
                        } else {
                            Symbol::Method(receiver.clone(), member.to_ascii_lowercase())
                        })
                    }
                    _ => None,
                }
            }
            _ => literal_name(&argument.value).and_then(|value| match function {
                "function_exists" => Some(Symbol::Function(value.to_ascii_lowercase())),
                "defined" => {
                    if let Some((class, _)) = value.split_once("::") {
                        scope
                            .known
                            .insert(Symbol::Class(class.to_ascii_lowercase()));
                    }
                    Some(Symbol::Constant(Symbol::constant_name(value)))
                }
                _ => None,
            }),
        };
        scope.known.extend(symbol);
    }

    pub fn class_probe(&mut self, expression: &Expression) -> Option<Symbol> {
        if let Some(name) = literal_name(expression) {
            return Some(Symbol::Class(name.to_ascii_lowercase()));
        }
        if let ExpressionKind::ClassConstant {
            class,
            name: MemberName::Identifier(member),
        } = &expression.kind
            && member.name.eq_ignore_ascii_case("class")
            && let ExpressionKind::Name(name) = &class.kind
        {
            return self
                .class_name(name, false)
                .map(|class| Symbol::Class(class.to_ascii_lowercase()));
        }
        None
    }

    pub fn condition_truth(&self, expression: &Expression, scope: &Scope) -> Option<bool> {
        if let Some(value) = crate::constants::value(expression, scope) {
            return Some(value.truth());
        }
        let mut expression = expression;
        let mut inverted = false;
        while let ExpressionKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } = &expression.kind
        {
            inverted = !inverted;
            expression = operand;
        }
        if let ExpressionKind::Isset(items) = &expression.kind {
            let facts: Vec<_> = items
                .iter()
                .map(|item| self.isset_truth(item, scope))
                .collect();
            if facts.contains(&Some(false)) {
                return Some(inverted);
            }
            if facts.iter().all(|fact| *fact == Some(true)) {
                return Some(!inverted);
            }
        }
        if let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
        } = &expression.kind
            && let ExpressionKind::Name(name) = &callee.kind
            && let Some(function) = self.existence_function(name)
            && let Some(argument) = arguments.first()
            && function == "is_null"
        {
            return self
                .nullish(&argument.value, scope)
                .map(|null| null != inverted);
        }
        if let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
        } = &expression.kind
            && let ExpressionKind::Name(name) = &callee.kind
            && let Some(function) = self.existence_function(name)
            && let Some(argument) = arguments.first()
            && let ExpressionKind::Literal(Literal::String(bytes)) = &argument.value.kind
            && let Ok(value) = std::str::from_utf8(bytes)
        {
            let value = value.trim_start_matches('\\').to_ascii_lowercase();
            let known = match function {
                "function_exists" => self.index.builtin_functions.contains(&value),
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                    self.index.builtin_classes.contains(&value)
                        && self.index.class(&value).is_some_and(|c| {
                            !c.location.file.starts_with("phpstorm-stubs/")
                                && match function {
                                    "interface_exists" => c.kind == ClassKind::Interface,
                                    "trait_exists" => c.kind == ClassKind::Trait,
                                    "enum_exists" => c.kind == ClassKind::Enum,
                                    _ => matches!(c.kind, ClassKind::Class | ClassKind::Enum),
                                }
                        })
                }
                _ => false,
            };
            if known {
                return Some(!inverted);
            }
        }
        None
    }
    fn isset_truth(&self, expression: &Expression, scope: &Scope) -> Option<bool> {
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            if name == "this" && (self.context.object || self.context.unbound_this.is_some()) {
                return self.context.object.then_some(true);
            }
            // Superglobals such as $_SESSION are optional at runtime; only use tracked writes.
            if let Some(variable) = scope.variables.get(name) {
                if variable.ty == ValueType::Null {
                    return Some(false);
                }
                if variable.definition == Definition::Definite && variable.ty.excludes_null() {
                    return Some(true);
                }
            } else if scope.get(name).definition == Definition::Absent && !scope.dynamic {
                return Some(false);
            }
        }
        None
    }

    fn existence_function(&self, name: &Name) -> Option<&'static str> {
        let candidates = self.context.names.resolve(name, UseKind::Function);
        let actual = candidates
            .iter()
            .find(|n| self.index.functions.contains_key(&n.to_ascii_lowercase()))?;
        if !self
            .index
            .builtin_functions
            .contains(&actual.to_ascii_lowercase())
        {
            return None;
        }
        match actual.to_ascii_lowercase().as_str() {
            "function_exists" => Some("function_exists"),
            "defined" => Some("defined"),
            "class_exists" => Some("class_exists"),
            "interface_exists" => Some("interface_exists"),
            "trait_exists" => Some("trait_exists"),
            "enum_exists" => Some("enum_exists"),
            "is_null" => Some("is_null"),
            "method_exists" => Some("method_exists"),
            "property_exists" => Some("property_exists"),
            _ => None,
        }
    }
}

fn literal_name(expression: &Expression) -> Option<&str> {
    if let ExpressionKind::Literal(Literal::String(bytes)) = &expression.kind {
        return std::str::from_utf8(bytes)
            .ok()
            .map(|name| name.trim_start_matches('\\'));
    }
    None
}
