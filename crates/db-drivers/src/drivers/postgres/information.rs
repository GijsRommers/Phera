//! `PDO::ATTR_SERVER_INFO` reads the backend and session settings.

use super::Postgres;

impl Postgres {
    pub fn connection_status(&self) -> &'static str {
        if self.is_broken() {
            "Bad connection."
        } else {
            "Connection OK; waiting to send."
        }
    }

    pub fn server_info(&self) -> String {
        let pid = self.client.pid();
        let encoding = self.client.parameter("client_encoding").unwrap_or_default();
        let superuser = self.client.parameter("is_superuser").unwrap_or_default();
        let user = self
            .client
            .parameter("session_authorization")
            .unwrap_or_default();
        let date_style = self.client.parameter("DateStyle").unwrap_or_default();
        format!(
            "PID: {pid}; Client Encoding: {encoding}; Is Superuser: {superuser}; \
            Session Authorization: {user}; Date Style: {date_style}"
        )
    }
}
