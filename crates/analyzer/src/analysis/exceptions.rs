//! Exception edges: `try { $x = call(); } catch (...) {}` may leave `$x` undefined.
//! Snapshots are taken at operations that may throw, rather than at every statement.

use super::*;
use super::{
    finally::{ExitKind, ExitPath},
    statements::Flow,
};

impl Analyzer<'_> {
    pub fn record_exception(&mut self, scope: &Scope) {
        if let Some(scopes) = self.exception_scopes.last_mut() {
            scopes.push(scope.clone());
        }
    }

    pub fn try_statement(
        &mut self,
        body: &[Statement],
        catches: &[Catch],
        finally: Option<&[Statement]>,
        scope: &mut Scope,
    ) -> Flow {
        let marks = self.exit_marks();
        self.return_scopes.push(Vec::new());
        self.exception_scopes.push(Vec::new());
        let flow = self.block(body, scope);
        let thrown = self.exception_scopes.pop().unwrap_or_default();
        let mut paths = Vec::new();
        if flow == Flow::Next {
            paths.push(ExitPath {
                kind: ExitKind::Live,
                scope: scope.clone(),
            });
        }
        self.exception_scopes.push(Vec::new());
        self.catch_paths(catches, &thrown, &mut paths);
        let escaping = self.exception_scopes.pop().unwrap_or_default();
        let returned = self.return_scopes.pop().unwrap_or_default();
        paths.extend(escaping.into_iter().map(|scope| ExitPath {
            kind: ExitKind::Exception,
            scope,
        }));
        paths.extend(returned.into_iter().map(|scope| ExitPath {
            kind: ExitKind::Return,
            scope,
        }));
        let catches_all = catches.iter().any(|c| {
            c.types.iter().any(|n| {
                self.context.names.resolve(n, UseKind::Class)[0].eq_ignore_ascii_case("Throwable")
            })
        });
        if !catches_all {
            paths.extend(thrown.into_iter().map(|scope| ExitPath {
                kind: ExitKind::Exception,
                scope,
            }));
        }
        paths.extend(self.take_controls(&marks));
        if let Some(finally) = finally {
            self.transfer_finally(finally, &mut paths);
        }
        self.restore_paths(paths, scope)
    }

    fn catch_paths(&mut self, catches: &[Catch], thrown: &[Scope], paths: &mut Vec<ExitPath>) {
        for catch in catches {
            for ty in &catch.types {
                self.class_name(ty, true);
            }
            if thrown.is_empty() {
                continue;
            }
            let mut scope = Scope::join(thrown);
            if let Some(name) = &catch.variable {
                let ty = catch
                    .types
                    .first()
                    .and_then(|t| self.class_name(t, false))
                    .map_or(ValueType::Mixed, ValueType::Class);
                scope.define(name, ty);
            }
            if self.block(&catch.body, &mut scope) == Flow::Next {
                paths.push(ExitPath {
                    kind: ExitKind::Live,
                    scope,
                });
            }
        }
    }
}
