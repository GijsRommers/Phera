//! `run -i 7` responds when PHP breaks or finishes. `break -i 8` can interrupt it.

use std::{io, rc::Rc};

use phera_extension_api::{LocalFuture, NativeError};
use quick_xml::escape::escape;

use crate::Vm;

use super::breakpoints::Breakpoints;
use super::execution::Position;
use super::features::Features;
use super::protocol::{Command, NAMESPACES, Wire};
use super::{DebugTimeLimit, Stream, Streams};

pub(crate) struct Session {
    wire: Wire,
    file: String,
    pending: Option<Command>,
    incoming: Option<Command>,
    disconnected: bool,
    polls: u16,
    pub(super) position: Option<Position>,
    pub(super) force_break: bool,
    pub(super) breakpoints: Breakpoints,
    pub(super) features: Features,
    pub(super) notifications: Vec<String>,
    pub(super) stop: Option<Stop>,
    pub(super) last_exception: Option<u32>,
    pub(super) streams: Rc<Streams>,
    mode: Resume,
    paused: bool,
    time_limit: Option<DebugTimeLimit>,
}

pub(super) struct Stop {
    pub(super) location: String,
    pub(super) unconditional: bool,
    pub(super) candidates: Vec<(u64, Option<Vec<u8>>)>,
}

enum Resume {
    Run,
    Into,
    Over(usize),
    Out(usize),
}

enum Action {
    Wait,
    Resume,
    Detach,
    Stop,
}

impl Session {
    pub(super) fn new(wire: Wire, file: String, time_limit: Option<DebugTimeLimit>) -> Self {
        Self {
            wire,
            file,
            pending: None,
            incoming: None,
            disconnected: false,
            polls: 0,
            position: None,
            force_break: false,
            breakpoints: Breakpoints::default(),
            features: Features::default(),
            notifications: Vec::new(),
            stop: None,
            last_exception: None,
            streams: Rc::new(Streams::default()),
            mode: Resume::Run,
            paused: false,
            time_limit,
        }
    }

    pub(super) async fn init(&mut self, idekey: &str) -> io::Result<()> {
        let file = escape(&self.file);
        let idekey = escape(idekey);
        let pid = std::process::id();
        let version = env!("CARGO_PKG_VERSION");
        let php = phera_extension_api::PHP_VERSION;
        self.wire.write(&format!(
            r#"<init {NAMESPACES} fileuri="{file}" language="PHP" xdebug:language_version="{php}" protocol_version="1.0" appid="{pid}" idekey="{idekey}"><engine version="{version}">Phera</engine></init>"#
        )).await
    }

    pub(super) fn stepping(&self, depth: usize) -> bool {
        match self.mode {
            Resume::Run => false,
            Resume::Into => true,
            Resume::Over(original) => depth <= original,
            Resume::Out(original) => depth < original,
        }
    }

    pub(super) fn poll_running(&mut self) -> bool {
        if self.pending.is_none() {
            return true;
        }
        self.polls = self.polls.wrapping_add(1);
        if self.polls.is_multiple_of(256) {
            match self.wire.try_read() {
                Ok(command) => self.incoming = command,
                Err(_) => self.disconnected = true,
            }
        }
        self.incoming.is_some() || self.disconnected || self.streams.pending()
    }

    async fn dispatch(
        &mut self,
        vm: &mut Vm,
        command: Command,
        running: bool,
    ) -> io::Result<Action> {
        let response = match command.name.as_str() {
            "feature_get" => self.features.get(&command),
            "stdout" => self.streams.command(&command, Stream::Stdout),
            "stderr" => self.streams.command(&command, Stream::Stderr),
            "feature_set" => self.features.set(&command),
            "status" => {
                let status = if running {
                    "running"
                } else if self.paused {
                    "break"
                } else {
                    "starting"
                };
                command.response(&format!(r#" status="{status}" reason="ok""#), "")
            }
            "run" | "step_into" | "step_over" | "step_out" if !running => {
                let depth = vm.call_stack.depth();
                self.mode = match command.name.as_str() {
                    "step_into" => Resume::Into,
                    "step_over" => Resume::Over(depth),
                    "step_out" => Resume::Out(depth),
                    _ => Resume::Run,
                };
                self.pending = Some(command);
                return Ok(Action::Resume);
            }
            "detach" | "stop" => {
                let action = if command.name == "stop" {
                    Action::Stop
                } else {
                    Action::Detach
                };
                let status = if matches!(action, Action::Stop) {
                    "stopped"
                } else {
                    "stopping"
                };
                self.wire
                    .write(&command.response(&format!(r#" status="{status}" reason="ok""#), ""))
                    .await?;
                return Ok(action);
            }
            "break" => {
                self.force_break = running;
                command.response(r#" success="1""#, "")
            }
            "breakpoint_set" | "breakpoint_list" | "breakpoint_get" | "breakpoint_remove"
            | "breakpoint_update" => {
                let response = self.breakpoints.command(
                    &command,
                    &self.file,
                    |file, line| vm.debug_line(file, line),
                    self.features.resolved_breakpoints,
                );
                if command.name == "breakpoint_set"
                    && self.features.notify_ok
                    && self.features.resolved_breakpoints
                    && let Some(breakpoint) = self.breakpoints.latest_resolution()
                {
                    self.wire.write(&format!(
                        r#"<notify {NAMESPACES} name="breakpoint_resolved">{breakpoint}</notify>"#,
                    )).await?;
                }
                response
            }
            _ if running => command.error(5, "command is unavailable while running"),
            _ => {
                vm.debug_inspection(&command, &self.features, &self.file)
                    .await
            }
        };
        self.wire.write(&response).await?;
        Ok(Action::Wait)
    }

    async fn poll(&mut self, vm: &mut Vm) -> io::Result<Action> {
        self.streams.flush(&mut self.wire).await?;
        if self.disconnected {
            return Ok(Action::Detach);
        }
        if let Some(command) = self.incoming.take() {
            let action = self.dispatch(vm, command, true).await?;
            if !matches!(action, Action::Wait) {
                return Ok(action);
            }
        }
        for notification in std::mem::take(&mut self.notifications) {
            self.wire.write(&notification).await?;
        }
        if let Some(mut stop) = self.stop.take() {
            let mut should_stop = stop.unconditional;
            for (id, expression) in stop.candidates {
                if let Some(expression) = expression
                    && !vm.debug_condition(&expression).await
                {
                    continue;
                }
                if let Some(details) = self.breakpoints.hit(id, self.features.resolved_breakpoints)
                {
                    should_stop = true;
                    if self.features.breakpoint_details {
                        stop.location.push_str(&details);
                    }
                }
            }
            if should_stop && let Some(command) = self.pending.take() {
                self.paused = true;
                self.wire
                    .write(&command.response(r#" status="break" reason="ok""#, &stop.location))
                    .await?;
            }
        }
        if self.pending.is_some() {
            return Ok(Action::Resume);
        }
        let _paused = self.time_limit.as_ref().map(DebugTimeLimit::pause);
        loop {
            let command = self.wire.read().await?;
            let action = self.dispatch(vm, command, false).await?;
            if !matches!(action, Action::Wait) {
                return Ok(action);
            }
        }
    }

    pub(super) async fn finish(&mut self) -> io::Result<()> {
        self.streams.flush(&mut self.wire).await?;
        if let Some(command) = self.pending.take() {
            self.wire
                .write(&command.response(r#" status="stopping" reason="ok""#, ""))
                .await?;
            let _paused = self.time_limit.as_ref().map(DebugTimeLimit::pause);
            let command = self.wire.read().await?;
            self.wire
                .write(&command.response(r#" status="stopped" reason="ok""#, ""))
                .await?;
        }
        Ok(())
    }
}

impl Vm {
    #[cold]
    pub(crate) fn debugger_commands(&mut self) -> LocalFuture<'_, Result<(), NativeError>> {
        Box::pin(async move {
            let Some(mut session) = self.debugger.take() else {
                return Ok(());
            };
            match session.poll(self).await {
                Ok(Action::Resume) => self.debugger = Some(session),
                Ok(Action::Stop) => {
                    self.output_writer.debugger = None;
                    self.release_frames_for_exit();
                    return Err(Self::debugger_exit());
                }
                Ok(Action::Detach | Action::Wait) | Err(_) => self.output_writer.debugger = None,
            }
            Ok(())
        })
    }
}
