//! Unary built-in calls written unqualified inside a namespace.
//!
//! ```php
//! namespace App;
//! if (is_null($value)) { ... }   // GuardBuiltin, then BuiltinUnary or the call
//! $n = count($this->items);
//! ```
//!
//! Inside `namespace App;` the call `is_null($value)` means `App\is_null` if a
//! function of that name is declared when the call site first runs, and the
//! global `is_null` otherwise; a site keeps what it found then. So such a call
//! compiles to both forms:
//!
//! ```text
//!     GuardBuiltin  App\is_null, call: general
//!     BuiltinUnary  is_null, value -> result      // the built-in's own instruction
//!     Jump          done
//! general:
//!     InitCall App\is_null; SendLocal value; Call; Move -> result
//! done:
//! ```
//!
//! The guard comes before the argument, as PHP finds the function before it
//! evaluates the arguments. The argument is compiled once for each form, so
//! only arguments that declare nothing qualify: variables, literals,
//! constants, and property, element and static property reads of those. A
//! closure, an arrow function or an anonymous class would be declared twice,
//! and keeps the general call alone.

use phera_ast::{Argument, Arguments, Expression, ExpressionKind, MemberName, Variable};
use phera_bytecode::{BuiltinUnary, Reg};

use crate::ir::{Instruction, Target};
use crate::{CompileError, Compiler};

use super::builtin_calls::builtin_unary;
use super::builtin_literals::defined_literal;

impl Compiler<'_> {
    /// Both forms of an unqualified unary built-in call in a namespace, or
    /// `None` for a call that keeps the general form alone (named or spread
    /// arguments, another argument count, a disabled function, an argument
    /// that declares something).
    pub(super) fn compile_namespaced_builtin(
        &mut self,
        resolved: &str,
        fallback: &str,
        arguments: &Arguments,
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
        let Some((operator, argument)) = self.guarded_operation(fallback, arguments) else {
            return Ok(None);
        };
        let destination = self.allocate_register()?;
        let guard = self.emit(
            Instruction::GuardBuiltin {
                name: resolved.to_owned(),
                fallback: Some(fallback.to_owned()),
                call: Target(0),
            },
            line,
        );
        let source = if operator == BuiltinUnary::Defined {
            match defined_literal(&argument.value) {
                Some(name) => self.literal(name, line)?,
                None => return Err(self.error("defined() without a literal name", line)),
            }
        } else {
            self.builtin_operand(&argument.value, line)?
        };
        self.emit(
            Instruction::NamespacedBuiltin {
                destination,
                operator,
                source,
            },
            line,
        );
        let done = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(guard, self.position());
        self.emit(
            Instruction::StartCall {
                name: resolved.to_owned(),
                fallback: Some(fallback.to_owned()),
            },
            line,
        );
        let result = self.compile_arguments(arguments, line)?;
        self.emit(
            Instruction::Move {
                destination,
                source: result,
            },
            line,
        );
        self.patch(done, self.position());
        Ok(Some(destination))
    }

    /// Both forms of an unqualified positional call to a direct native function
    /// in a namespace: `GuardBuiltin`, then the global builtin or direct native
    /// call (which `direct_calls` lowers to `CallNativeDirect` / `PureNative`),
    /// then the general fallback call.
    pub(super) fn compile_namespaced_direct_native(
        &mut self,
        resolved: &str,
        fallback: &str,
        arguments: &Arguments,
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
        let Arguments::List(args) = arguments else {
            return Ok(None);
        };
        if args
            .iter()
            .any(|argument| argument.spread || argument.name.is_some())
        {
            return Ok(None);
        }
        let lower = fallback.to_ascii_lowercase();
        if lower == "assert" {
            return Ok(None);
        }
        let Some(enabled) = self.native_functions else {
            return Ok(None);
        };
        let Some(crate::NativeFunction::Direct {
            positional_by_value,
        }) = (enabled.lookup)(&lower)
        else {
            return Ok(None);
        };
        if args.len() > positional_by_value {
            return Ok(None);
        }
        let mut budget = 32usize;
        if !args
            .iter()
            .all(|argument| declares_nothing_bounded(&argument.value, &mut budget))
        {
            return Ok(None);
        }
        let destination = self.allocate_register()?;
        let guard = self.emit(
            Instruction::GuardBuiltin {
                name: resolved.to_owned(),
                fallback: Some(fallback.to_owned()),
                call: Target(0),
            },
            line,
        );
        let fast_mark = self.mark_temporaries();
        let fast_result =
            if let Some(result) = self.compile_builtin_call(fallback, arguments, line)? {
                result
            } else {
                self.emit(
                    Instruction::StartCall {
                        name: fallback.to_owned(),
                        fallback: None,
                    },
                    line,
                );
                self.compile_arguments(arguments, line)?
            };
        self.emit(
            Instruction::Move {
                destination,
                source: fast_result,
            },
            line,
        );
        self.release_temporaries(fast_mark);
        let done = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(guard, self.position());
        let slow_mark = self.mark_temporaries();
        self.emit(
            Instruction::StartCall {
                name: resolved.to_owned(),
                fallback: Some(fallback.to_owned()),
            },
            line,
        );
        let result = self.compile_arguments(arguments, line)?;
        self.emit(
            Instruction::Move {
                destination,
                source: result,
            },
            line,
        );
        self.release_temporaries(slow_mark);
        self.patch(done, self.position());
        Ok(Some(destination))
    }

    /// The built-in a call of the global `name` with these arguments runs as
    /// one instruction, and its one argument.
    pub(super) fn guarded_operation<'a>(
        &self,
        name: &str,
        arguments: &'a Arguments,
    ) -> Option<(BuiltinUnary, &'a Argument)> {
        let Arguments::List(arguments) = arguments else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if argument.spread || argument.name.is_some() {
            return None;
        }
        let name = name.to_ascii_lowercase();
        let enabled = self.native_functions?;
        (enabled.lookup)(&name)?;
        let operator = builtin_unary(&name)?;
        if operator == BuiltinUnary::Defined && defined_literal(&argument.value).is_none() {
            return None;
        }
        declares_nothing(&argument.value).then_some((operator, argument))
    }
}

/// Whether compiling `expression` twice is the same as compiling it once:
/// it is made only of variables, literals, constants and reads of
/// properties, elements and static properties of those. Walked with a stack
/// of its own, so a deep expression needs no deep Rust stack.
fn declares_nothing(expression: &Expression) -> bool {
    let mut budget = usize::MAX;
    declares_nothing_bounded(expression, &mut budget)
}

fn declares_nothing_bounded(expression: &Expression, budget: &mut usize) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        match &expression.kind {
            ExpressionKind::Literal(_)
            | ExpressionKind::Name(_)
            | ExpressionKind::MagicConstant(_)
            | ExpressionKind::Variable(Variable::Named(_)) => {}
            ExpressionKind::Property { target, name, .. } => {
                pending.push(target);
                if let MemberName::Expression(name) = name {
                    pending.push(name);
                }
            }
            ExpressionKind::Index { target, index } => {
                pending.push(target);
                match index {
                    Some(index) => pending.push(index),
                    // `$a[]` cannot be read: the general call raises the error.
                    None => return false,
                }
            }
            ExpressionKind::StaticProperty {
                class,
                name: Variable::Named(_),
                ..
            }
            | ExpressionKind::ClassConstant {
                class,
                name: MemberName::Identifier(_),
            } => pending.push(class),
            ExpressionKind::Array(items) => {
                for item in items {
                    if item.by_reference || item.spread {
                        return false;
                    }
                    if let Some(key) = &item.key {
                        pending.push(key);
                    }
                    pending.push(&item.value);
                }
            }
            ExpressionKind::Unary { operand, .. } | ExpressionKind::Cast { operand, .. } => {
                pending.push(operand);
            }
            ExpressionKind::Binary { left, right, .. } => {
                pending.push(left);
                pending.push(right);
            }
            _ => return false,
        }
    }
    true
}
