//! `parse_ini_string('path = "${APP_DIR:-/tmp}/cache"')` expands variables iteratively.

use phera_extension_api::{Context, value::to_php_string};

use super::{parser::Parser, token::Token};
use crate::environment::RequestEnvironment;

pub(super) struct Fragment {
    pub(super) bytes: Vec<u8>,
    pub(super) numeric: bool,
}

impl Fragment {
    fn text(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            numeric: false,
        }
    }
}

#[derive(Default)]
struct Concatenation(Option<Fragment>);

impl Concatenation {
    fn append(&mut self, next: Fragment) {
        if let Some(current) = &mut self.0 {
            current.bytes.extend(next.bytes);
            current.numeric = false;
        } else {
            self.0 = Some(next);
        }
    }

    fn finish(self) -> Fragment {
        self.0.unwrap_or_else(|| Fragment::text(Vec::new()))
    }
}

enum Assembly<'a> {
    Quoted(Concatenation),
    Fallback(&'a [u8], Concatenation),
}

impl<'a> Parser<'a> {
    /// Collect the grammar's adjacent string fragments and unfold nested substitutions.
    pub(super) fn fragments(
        &mut self,
        context: &mut dyn Context,
        section: bool,
    ) -> Result<Option<Fragment>, String> {
        let mut scopes = Vec::new();
        let mut root = Concatenation::default();
        loop {
            let fragment = match (&self.lookahead, scopes.last()) {
                (Token::Variable, _) => {
                    self.take();
                    let Token::VarName(name) = self.lookahead else {
                        return Err(self.syntax(", expecting TC_VARNAME"));
                    };
                    self.take();
                    match self.lookahead {
                        Token::Fallback => {
                            self.take();
                            scopes.push(Assembly::Fallback(name, Concatenation::default()));
                            if scopes.len() >= super::MAX_DEPTH {
                                return Err("memory exhausted".into());
                            }
                            continue;
                        }
                        Token::Punctuation(b'}') => {
                            self.take();
                            variable(context, name, Concatenation::default())
                        }
                        _ => return Err(self.syntax(", expecting TC_FALLBACK or '}'")),
                    }
                }
                (Token::Quoted(_), Some(Assembly::Quoted(_))) => {
                    let Token::Quoted(bytes) = self.take() else {
                        return Err(self.syntax(""));
                    };
                    Fragment::text(bytes.into_owned())
                }
                (Token::Punctuation(b'"'), Some(Assembly::Quoted(_))) => {
                    self.take();
                    let Some(Assembly::Quoted(builder)) = scopes.pop() else {
                        return Err(self.syntax(""));
                    };
                    builder.finish()
                }
                (Token::Punctuation(b'}'), Some(Assembly::Fallback(_, _))) => {
                    self.take();
                    let Some(Assembly::Fallback(name, builder)) = scopes.pop() else {
                        return Err(self.syntax(""));
                    };
                    variable(context, name, builder)
                }
                (_, Some(Assembly::Quoted(_))) => {
                    return Err(
                        self.syntax(", expecting TC_DOLLAR_CURLY or TC_QUOTED_STRING or '\"'")
                    );
                }
                (Token::Punctuation(b'"'), _) => {
                    self.take();
                    scopes.push(Assembly::Quoted(Concatenation::default()));
                    continue;
                }
                (Token::Number(bytes), _) => {
                    let bytes = bytes.to_vec();
                    self.take();
                    Fragment {
                        bytes,
                        numeric: true,
                    }
                }
                (Token::Constant(bytes), scope) => {
                    let bytes = if section && scope.is_none() {
                        bytes.to_vec()
                    } else {
                        constant(context, bytes)
                    };
                    self.take();
                    Fragment::text(bytes)
                }
                (Token::Raw(bytes) | Token::Text(bytes) | Token::Space(bytes), _) => {
                    let bytes = bytes.to_vec();
                    self.take();
                    Fragment::text(bytes)
                }
                (_, Some(Assembly::Fallback(_, _))) => return Err(self.syntax(", expecting '}'")),
                (_, None) => return Ok(root.0),
            };
            match scopes.last_mut() {
                Some(Assembly::Quoted(builder) | Assembly::Fallback(_, builder)) => {
                    builder.append(fragment);
                }
                None => root.append(fragment),
            }
        }
    }
}

fn constant(context: &mut dyn Context, name: &[u8]) -> Vec<u8> {
    if let Ok(name) = std::str::from_utf8(name)
        && let Some(value) = context.named_constant(name)
        && let Ok(string) = to_php_string(&value, context.diagnostics())
    {
        return string.as_bytes().to_vec();
    }
    name.to_vec()
}

fn variable(context: &mut dyn Context, name: &[u8], fallback: Concatenation) -> Fragment {
    let env = context.state().get_or_default::<RequestEnvironment>();
    if let Some(value) = env.lookup(name) {
        return Fragment::text(value.to_vec());
    }
    let mut value = fallback.finish();
    if let Some(end) = value.bytes.iter().position(|&byte| byte == 0) {
        value.bytes.truncate(end);
    }
    value.numeric = false;
    value
}
