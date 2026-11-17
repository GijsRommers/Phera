//! Exception edges: `try { $x = call(); } catch (...) {}` may leave `$x` undefined.
//! Snapshots are taken at operations that may throw, rather than at every statement.

use super::statements::Flow;
use super::*;

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
        self.exception_scopes.push(Vec::new());
        let body_flow = self.block(body, scope);
        let thrown = self.exception_scopes.pop().unwrap_or_default();
        let caught = Scope::join(&thrown);
        let mut live = Vec::new();
        let mut all = vec![scope.clone()];
        if body_flow == Flow::Next {
            live.push(scope.clone());
        }
        for catch in catches {
            for ty in &catch.types {
                self.class_name(ty, true);
            }
            if thrown.is_empty() {
                continue;
            }
            let mut branch = caught.clone();
            if let Some(name) = &catch.variable {
                let ty = catch
                    .types
                    .first()
                    .and_then(|t| self.class_name(t, false))
                    .map_or(ValueType::Mixed, ValueType::Class);
                branch.define(name, ty);
            }
            if self.block(&catch.body, &mut branch) == Flow::Next {
                live.push(branch.clone());
            }
            all.push(branch);
        }
        // Unmatched exceptions also traverse finally, but cannot continue after try.
        all.extend(thrown.iter().cloned());
        if let Some(finally) = finally
            && let Some(flow) = self.finally(finally, &all, &mut live)
        {
            return flow;
        }
        for thrown in &thrown {
            self.record_exception(thrown);
        }
        if live.is_empty() {
            Flow::Terminal
        } else {
            *scope = Scope::join(&live);
            Flow::Next
        }
    }

    fn finally(&mut self, body: &[Statement], all: &[Scope], live: &mut [Scope]) -> Option<Flow> {
        let joined = Scope::join(all);
        let mut final_scope = joined.clone();
        let flow = self.block(body, &mut final_scope);
        if flow != Flow::Next {
            return Some(flow);
        }
        for path in live {
            // Re-analyse with the live scope for transfer, while keeping diagnostics only
            // from the join above. This also carries unset and changed property types.
            let before_diagnostics = self.diagnostics.len();
            self.block(body, path);
            self.diagnostics.truncate(before_diagnostics);
        }
        None
    }
}
