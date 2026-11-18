//! Finally transfers every exit: `try { break; } finally { $x = 1; }` defines `$x`.
//! Return, throw, break and continue paths keep their identity through cleanup.

use super::*;
use super::{control::ControlKind, statements::Flow};

pub enum ExitKind {
    Live,
    Return,
    Exception,
    Break(usize),
    Continue(usize),
}
pub struct ExitPath {
    pub kind: ExitKind,
    pub scope: Scope,
}
pub struct ExitMarks {
    controls: Vec<(usize, usize)>,
    returns: usize,
    exceptions: usize,
}

impl Analyzer<'_> {
    pub fn exit_marks(&self) -> ExitMarks {
        ExitMarks {
            controls: self
                .controls
                .iter()
                .map(|f| (f.breaks.len(), f.continues.len()))
                .collect(),
            returns: self.return_scopes.last().map_or(0, Vec::len),
            exceptions: self.exception_scopes.last().map_or(0, Vec::len),
        }
    }

    pub fn take_controls(&mut self, marks: &ExitMarks) -> Vec<ExitPath> {
        let mut paths = Vec::new();
        for (index, (frame, (breaks, continues))) in
            self.controls.iter_mut().zip(&marks.controls).enumerate()
        {
            paths.extend(
                frame
                    .breaks
                    .split_off(*breaks)
                    .into_iter()
                    .map(|scope| ExitPath {
                        kind: ExitKind::Break(index),
                        scope,
                    }),
            );
            paths.extend(
                frame
                    .continues
                    .split_off(*continues)
                    .into_iter()
                    .map(|scope| ExitPath {
                        kind: ExitKind::Continue(index),
                        scope,
                    }),
            );
        }
        paths
    }

    fn discard_exits(&mut self, marks: &ExitMarks) {
        for (frame, (breaks, continues)) in self.controls.iter_mut().zip(&marks.controls) {
            frame.breaks.truncate(*breaks);
            frame.continues.truncate(*continues);
        }
        if let Some(returns) = self.return_scopes.last_mut() {
            returns.truncate(marks.returns);
        }
        if let Some(exceptions) = self.exception_scopes.last_mut() {
            exceptions.truncate(marks.exceptions);
        }
    }

    pub fn transfer_finally(&mut self, body: &[Statement], paths: &mut Vec<ExitPath>) {
        if paths.is_empty() {
            return;
        }
        let mut joined = Scope::join(&paths.iter().map(|p| p.scope.clone()).collect::<Vec<_>>());
        let marks = self.exit_marks();
        self.block(body, &mut joined);
        self.discard_exits(&marks);
        let diagnostics = self.diagnostics.len();
        // Diagnostic checking uses the join; transfer uses each exit's own bindings.
        paths.retain_mut(|path| self.block(body, &mut path.scope) == Flow::Next);
        self.diagnostics.truncate(diagnostics);
    }

    pub fn restore_paths(&mut self, paths: Vec<ExitPath>, scope: &mut Scope) -> Flow {
        let mut live = Vec::new();
        for path in paths {
            match path.kind {
                ExitKind::Live => live.push(path.scope),
                ExitKind::Return => self.record_return(&path.scope),
                ExitKind::Exception => self.record_exception(&path.scope),
                ExitKind::Break(index) => {
                    if let Some(frame) = self.controls.get_mut(index) {
                        frame.breaks.push(path.scope);
                    }
                }
                ExitKind::Continue(index) => {
                    if let Some(frame) = self.controls.get_mut(index)
                        && frame.kind == ControlKind::Loop
                    {
                        frame.continues.push(path.scope);
                    }
                }
            }
        }
        Self::finish_paths(live, scope)
    }

    pub fn record_return(&mut self, scope: &Scope) {
        if let Some(returns) = self.return_scopes.last_mut() {
            returns.push(scope.clone());
        }
    }
}
