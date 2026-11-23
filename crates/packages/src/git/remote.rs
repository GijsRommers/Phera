//! Validate Git transport URLs and provide host-scoped credentials.
use crate::{network::Network, paths};
use anyhow::{Context, Result, bail};
use reqwest::Url;
use std::path::Path;
use std::process::Command;

pub(super) struct Remote {
    pub(super) url: String,
    https: Option<Url>,
    local: bool,
    allow_http: bool,
}

impl Remote {
    pub(super) fn new(project: &Path, address: &str, network: &Network) -> Result<Self> {
        let allow_http = network.allows_http();
        if address.starts_with("https://") || address.starts_with("http://") {
            let url = Url::parse(address).context("Invalid HTTP Git repository URL.")?;
            if url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                bail!("Use a Git repository URL without credentials, a query or a fragment.");
            }
            if url.scheme() == "http" && !allow_http {
                bail!("Refusing HTTP Git. Configure HTTPS or explicitly disable secure-http.");
            }
            return Ok(Self {
                url: url.to_string(),
                https: (url.scheme() == "https").then_some(url),
                local: false,
                allow_http,
            });
        }
        if address.starts_with("ssh://") || address.contains('@') {
            let host = if address.starts_with("ssh://") {
                let url = Url::parse(address).context("Invalid SSH Git repository URL.")?;
                if (!url.username().is_empty() && !safe_ssh_name(url.username()))
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    bail!("Use an SSH Git URL with an SSH account and no embedded password.");
                }
                url.host_str().map(str::to_owned)
            } else {
                address.split_once('@').and_then(|(user, tail)| {
                    safe_ssh_name(user)
                        .then_some(tail)?
                        .split_once(':')
                        .map(|(host, _)| host.to_owned())
                })
            };
            if !host.as_deref().is_some_and(safe_ssh_name) {
                bail!("Invalid SSH Git repository host.");
            }
            return Ok(Self {
                url: address.into(),
                https: None,
                local: false,
                allow_http,
            });
        }
        let path = paths::inside(project, address)
            .context("Git requires an HTTPS or SSH URL, or a repository inside the project.")?;
        if !path.starts_with(project) {
            bail!("Local Git repositories must stay inside the project.");
        }
        Ok(Self {
            url: path.to_string_lossy().into_owned(),
            https: None,
            local: true,
            allow_http,
        })
    }

    pub(super) fn command(&self, network: &Network) -> Result<Command> {
        let mut command = Command::new("git");
        command.args([
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "protocol.git.allow=never",
        ]);
        if !self.allow_http {
            command.args(["-c", "protocol.http.allow=never"]);
        }
        command.args([
            "-c",
            if self.local {
                "protocol.file.allow=always"
            } else {
                "protocol.file.allow=never"
            },
        ]);
        command.args([
            "-c",
            "http.followRedirects=false",
            "-c",
            "http.sslVerify=true",
        ]);
        command.args(["-c", "core.hooksPath=/dev/null"]);
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1");
        if std::env::var_os("GIT_SSH_COMMAND").is_none() {
            command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=15");
        }
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_CONFIG_COUNT",
            "GIT_SSL_NO_VERIFY",
            "COMPOSER_AUTH",
        ] {
            command.env_remove(key);
        }
        for key in [
            "GIT_TRACE",
            "GIT_TRACE_CURL",
            "GIT_CURL_VERBOSE",
            "GIT_TRACE_PACKET",
            "GIT_TRACE2",
            "GIT_TRACE2_EVENT",
        ] {
            command.env(key, "0");
        }
        if let Some(url) = &self.https
            && let Some(header) = network.git_header(url)?
        {
            command
                .env("GIT_CONFIG_COUNT", "1")
                .env(
                    "GIT_CONFIG_KEY_0",
                    format!("http.{}/.extraHeader", url.origin().ascii_serialization()),
                )
                .env("GIT_CONFIG_VALUE_0", header);
        }
        Ok(command)
    }
}

fn safe_ssh_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
