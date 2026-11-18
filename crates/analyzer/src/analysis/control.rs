//! Nonlocal control edges: `if ($stop) { break; }` preserves that branch's scope.
//! Frames distinguish loop continues from switch exits, including `break 2`.

use super::statements::Flow;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Loop,
    Switch,
}

pub struct ControlFrame {
    pub kind: ControlKind,
    pub breaks: Vec<Scope>,
    pub continues: Vec<Scope>,
}

impl ControlFrame {
    pub fn new(kind: ControlKind) -> Self {
        Self {
            kind,
            breaks: Vec::new(),
            continues: Vec::new(),
        }
    }
}

impl Analyzer<'_> {
    pub fn control_exit(&mut self, levels: u32, continuing: bool, scope: &Scope) -> Flow {
        let Some(position) = self.controls.len().checked_sub(levels as usize) else {
            return Flow::Terminal;
        };
        let frame = &mut self.controls[position];
        if continuing && frame.kind == ControlKind::Loop {
            frame.continues.push(scope.clone());
            Flow::Continue
        } else {
            frame.breaks.push(scope.clone());
            Flow::Break
        }
    }

    pub fn control_body(&mut self, body: &[Statement], scope: &mut Scope) -> (Flow, ControlFrame) {
        self.controls.push(ControlFrame::new(ControlKind::Loop));
        let flow = self.block(body, scope);
        let frame = self
            .controls
            .pop()
            .unwrap_or_else(|| ControlFrame::new(ControlKind::Loop));
        (flow, frame)
    }

    pub fn finish_paths(paths: Vec<Scope>, scope: &mut Scope) -> Flow {
        if paths.is_empty() {
            Flow::Terminal
        } else {
            *scope = Scope::join(&paths);
            Flow::Next
        }
    }
}
