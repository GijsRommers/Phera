//! Bounded rustls downloads with host-scoped Composer credentials.
//!
//! `COMPOSER_AUTH={"http-basic":{"repo.example":{"username":"u","password":"p"}}}`
//! authenticates only that host, including when a download redirects elsewhere.

use crate::Project;
use anyhow::{Context, Result, bail};
use reqwest::Url;
use reqwest::blocking::{Client, Response};
use reqwest::header::{AUTHORIZATION, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::time::Duration;

const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;

pub(crate) struct Network {
    client: Client,
    auth: Value,
    secure_http: bool,
}

impl Network {
    pub(crate) fn new(project: &Project) -> Result<Self> {
        let mut auth = Value::Object(Default::default());
        let file = project.directory.join("auth.json");
        if file.is_file() {
            auth = serde_json::from_slice(&fs::read(file)?)
                .context("Invalid project auth.json; correct its JSON syntax.")?;
        }
        if let Ok(environment) = std::env::var("COMPOSER_AUTH") {
            let extra: Value = serde_json::from_str(&environment)
                .map_err(|_| anyhow::anyhow!("Invalid COMPOSER_AUTH JSON."))?;
            if let (Some(target), Some(extra)) = (auth.as_object_mut(), extra.as_object()) {
                for (kind, values) in extra {
                    let hosts = target
                        .entry(kind)
                        .or_insert_with(|| Value::Object(Default::default()));
                    if let (Some(hosts), Some(values)) = (hosts.as_object_mut(), values.as_object())
                    {
                        hosts.extend(values.clone());
                    }
                }
            }
        }
        let client = Client::builder()
            .use_preconfigured_tls((*phera_io_pool::tls::webpki_client()?).clone())
            .user_agent("Phera/0.1 Composer-compatible")
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            auth,
            secure_http: project.data["config"]["secure-http"] != false,
        })
    }

    pub(crate) fn response(
        &self,
        address: &str,
        etag: Option<&str>,
        modified: Option<&str>,
    ) -> Result<Response> {
        let mut url = Url::parse(address).map_err(|_| anyhow::anyhow!("Invalid download URL."))?;
        let original_origin = url.origin();
        for _ in 0..10 {
            if !matches!(url.scheme(), "https" | "http")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                bail!("Use an HTTP(S) URL and put credentials in auth.json.");
            }
            let host = url.host_str().context("Download URL has no host.")?;
            if url.scheme() == "http" && self.secure_http {
                bail!(
                    "Refusing HTTP to {host}. Use HTTPS or explicitly set config.secure-http to false."
                );
            }
            let mut request = self.client.get(url.clone());
            if url.scheme() == "https" && url.origin() == original_origin {
                let credentials = &self.auth["http-basic"][host];
                if let Some(username) = credentials["username"].as_str() {
                    request = request.basic_auth(username, credentials["password"].as_str());
                }
                if let Some(token) = self.auth["bearer"][host].as_str() {
                    let mut header = HeaderValue::from_str(&format!("Bearer {token}"))
                        .map_err(|_| anyhow::anyhow!("Invalid bearer credential for {host}."))?;
                    header.set_sensitive(true);
                    request = request.header(AUTHORIZATION, header);
                }
            }
            if let Some(value) = etag {
                request = request.header(IF_NONE_MATCH, value);
            }
            if let Some(value) = modified {
                request = request.header(IF_MODIFIED_SINCE, value);
            }
            let response = request
                .send()
                .map_err(|_| anyhow::anyhow!("Download from {host} failed; check connectivity."))?;
            if response.status().is_redirection() && response.status().as_u16() != 304 {
                let location = response
                    .headers()
                    .get("location")
                    .context("Redirect has no location.")?
                    .to_str()
                    .context("Invalid redirect location.")?;
                let next = url.join(location).context("Invalid redirect URL.")?;
                if url.scheme() == "https" && next.scheme() != "https" {
                    bail!("Refusing an insecure redirect from {host}.");
                }
                url = next;
                continue;
            }
            return Ok(response);
        }
        bail!("Too many download redirects; check the package repository.")
    }

    pub(crate) fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.response(url, None, None)?;
        if !response.status().is_success() {
            bail!(
                "Download returned HTTP {}; check repository access.",
                response.status()
            );
        }
        bounded(response)
    }

    pub(crate) fn packagist_advisories(&self, packages: &[&str]) -> Result<Value> {
        let form: Vec<_> = packages
            .iter()
            .enumerate()
            .map(|(index, name)| (format!("packages[{index}]"), name))
            .collect();
        let response = self
            .client
            .post("https://packagist.org/api/security-advisories/")
            .timeout(Duration::from_secs(10))
            .form(&form)
            .send()
            .map_err(|_| anyhow::anyhow!("Could not reach the Packagist advisories API."))?;
        if !response.status().is_success() {
            bail!("Packagist advisories returned HTTP {}.", response.status());
        }
        serde_json::from_slice(&bounded(response)?).context("Invalid Packagist advisory JSON.")
    }
}

pub(crate) fn bounded(response: Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD)
    {
        bail!("Package download exceeds the 512 MiB safety limit.");
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("Package download was interrupted; retry installation."))?;
    if bytes.len() as u64 > MAX_DOWNLOAD {
        bail!("Package download exceeds the 512 MiB safety limit.");
    }
    Ok(bytes)
}
