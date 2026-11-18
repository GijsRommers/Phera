//! Send an idempotent test request to one of the two web runtimes.

use std::time::Duration;

use phera_extension_api::testing::serve::Server;

pub fn request(
    server: &Server,
    runtime: &str,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> String {
    for attempt in 0..3 {
        let response = server.request(method, target, headers, body);
        if !response.is_empty() {
            assert!(
                response.contains("\r\n\r\n"),
                "{runtime} {method} {target} returned incomplete HTTP: {response:?}"
            );
            return response;
        }
        if attempt < 2 {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    panic!("{runtime} {method} {target} closed without sending an HTTP response");
}
