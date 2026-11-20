//! Resolve a Composer credential for one HTTPS origin.

use crate::Project;
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Url, header::HeaderValue};
use serde_json::{Value, json};
use std::fs;

pub(super) struct Credentials(Value);

impl Credentials {
    pub(super) fn read(project: &Project) -> Result<Self> {
        let mut result = json!({});
        if let Some(home) = std::env::var_os("COMPOSER_HOME") {
            let home = std::path::PathBuf::from(home);
            read(&home.join("auth.json"), &mut result)?;
        }
        read(&project.directory.join("auth.json"), &mut result)?;
        if let Ok(environment) = std::env::var("COMPOSER_AUTH") {
            let extra = serde_json::from_str(&environment)
                .map_err(|_| anyhow::anyhow!("Invalid COMPOSER_AUTH JSON."))?;
            merge(&mut result, extra)?;
        }
        Ok(Self(result))
    }

    pub(super) fn header(
        &self,
        url: &Url,
        git: bool,
    ) -> Result<Option<(&'static str, HeaderValue)>> {
        let Some(host) = url.host_str().filter(|_| url.scheme() == "https") else {
            return Ok(None);
        };
        let authority = url.port().map(|port| format!("{host}:{port}"));
        let configured = |method: &str| {
            let entries = &self.0[method];
            authority
                .as_ref()
                .and_then(|authority| entries.get(authority))
                .unwrap_or(&entries[host])
        };
        let mut name = "authorization";
        let value = if let Some(token) = configured("bearer").as_str() {
            format!("Bearer {token}")
        } else if let Some(token) = configured("github-oauth").as_str() {
            if git {
                basic("x-access-token", token)
            } else {
                format!("Bearer {token}")
            }
        } else if let Some(token) = configured("gitlab-oauth").as_str() {
            if git {
                basic("oauth2", token)
            } else {
                format!("Bearer {token}")
            }
        } else if let Some(token) = configured("gitlab-token").as_str() {
            if git {
                basic("oauth2", token)
            } else {
                name = "private-token";
                token.to_owned()
            }
        } else if let (Some(username), Some(token)) = (
            configured("gitlab-token")["username"].as_str(),
            configured("gitlab-token")["token"].as_str(),
        ) {
            basic(username, token)
        } else if let Some(username) = configured("http-basic")["username"].as_str() {
            basic(
                username,
                configured("http-basic")["password"].as_str().unwrap_or(""),
            )
        } else {
            return Ok(None);
        };
        let mut value = HeaderValue::from_str(&value)
            .map_err(|_| anyhow::anyhow!("Invalid credential for {host}."))?;
        value.set_sensitive(true);
        Ok(Some((name, value)))
    }
}

fn read(path: &std::path::Path, result: &mut Value) -> Result<()> {
    match fs::read(path) {
        Ok(bytes) => {
            let data: Value = serde_json::from_slice(&bytes).context("Invalid auth.json.")?;
            merge(result, data)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn merge(result: &mut Value, data: Value) -> Result<()> {
    let Some(entries) = data.as_object() else {
        bail!("Composer authentication settings must be a JSON object.");
    };
    for (method, values) in entries {
        let Some(values) = values.as_object() else {
            continue;
        };
        if result[method].is_null() {
            result[method] = json!({});
        }
        if let Some(hosts) = result[method].as_object_mut() {
            hosts.extend(values.clone());
        }
    }
    Ok(())
}

fn basic(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_sensitive_and_scoped_to_the_configured_https_host() {
        let credentials = Credentials(json!({
            "github-oauth": {"github.example": "fixture"},
            "gitlab-token": {"gitlab.example": "local-token"},
            "http-basic": {"repo.example:9443": {"username":"test", "password":"fixture"}},
        }));
        let header = |address, git| {
            credentials
                .header(&Url::parse(address).unwrap(), git)
                .unwrap()
        };
        let (_, value) = header("https://github.example/packages.json", false).unwrap();
        assert!(value.is_sensitive());
        assert_eq!(value, "Bearer fixture");
        assert!(header("https://api.github.example/packages.json", false).is_none());
        assert!(header("http://github.example/packages.json", true).is_none());
        let (name, _) = header("https://gitlab.example/packages.json", false).unwrap();
        assert_eq!(name, "private-token");
        let (name, _) = header("https://gitlab.example/project.git", true).unwrap();
        assert_eq!(name, "authorization");
        assert!(header("https://repo.example/repo", true).is_none());
        assert!(header("https://repo.example:9443/repo", true).is_some());
    }
}
