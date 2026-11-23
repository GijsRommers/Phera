//! Symfony under `phera serve` with the warm-up before the snapshot.
//!
//! ```php
//! // What Phera's runtime for symfony/runtime runs once per worker:
//! $kernel->boot();
//! $this->warmUp($kernel);    // the event dispatcher, its listeners still lazy
//! Phera\snapshot();
//! $response = $kernel->handle(Request::createFromGlobals());   // every request
//! ```
//!
//! The warm-up may change what a request costs and nothing else. Every test
//! asks the same application three ways: `phera serve`,
//! `phera serve --no-service-warmup` (as `PHERA_SERVICE_WARMUP=0`), and PHP's
//! own server.
//!
//! The application is `tests/warmup_symfony/`; its packages come from an
//! installed Symfony application with Doctrine, named by `PHERA_SYMFONY_APP`
//! (see `framework_support`). Without it the tests are skipped.

mod framework_support;

use std::path::Path;
use std::process::Command;

use framework_support::{Answer, Project, get, installed_application};
use phera_extension_api::testing::{oracle, serve::Server};

/// The application, and the three servers answering for it.
struct Symfony {
    warm: Server,
    plain: Server,
    php: Server,
    // Dropped last: the servers run in it.
    project: Project,
}

impl Symfony {
    fn start() -> Option<Symfony> {
        let installed = installed_application("PHERA_SYMFONY_APP")?;
        let php = oracle::find_php().expect("PHP 8.5 oracle");
        let skeleton = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/warmup_symfony");
        let project = Project::new(&skeleton, &installed);
        let database = project.root.join("database/test.sqlite");
        project.write(
            ".env",
            &format!(
                "APP_ENV=prod\nAPP_DEBUG=0\nAPP_SECRET=0123456789abcdef0123456789abcdef\n\
                 DATABASE_URL=\"sqlite:///{}\"\n",
                database.display()
            ),
        );
        let seeded = Command::new(&php)
            .arg("-n")
            .arg(project.root.join("database/seed.php"))
            .arg(&database)
            .status()
            .expect("run PHP");
        assert!(seeded.success(), "the test database was not created");

        let root = project.document_root();
        // PHP answers first: its first request compiles the container and
        // warms the cache in var/, as a deployment's build step does, and
        // both Phera servers then boot from that cache.
        let php = Server::start_php(&php, &root, 1);
        assert_eq!(
            get(&php, "/api/ping", &[]).status,
            200,
            "PHP cannot run the application"
        );
        let phera = Path::new(env!("CARGO_BIN_EXE_phera"));
        Some(Symfony {
            warm: Server::start(phera, &root, 1, &[]),
            plain: Server::start(phera, &root, 1, &[("PHERA_SERVICE_WARMUP", "0")]),
            php,
            project,
        })
    }

    /// One more server, which handled `paths` as requests before its
    /// snapshot (`PHERA_WARMUP`, what `--warmup` sets), and the file its
    /// log is written to.
    fn warmed_by(&self, paths: &str) -> (Server, std::path::PathBuf) {
        let log = self.project.root.join("var/server.log");
        let server = Server::start_with(
            Path::new(env!("CARGO_BIN_EXE_phera")),
            &self.project.document_root(),
            1,
            &[("PHERA_WARMUP", paths)],
            &[],
            std::fs::File::create(&log).expect("create the server's log"),
        );
        (server, log)
    }

    /// The answer all three give to `target`, which must be the same one.
    fn same_answer(&self, target: &str, headers: &[(&str, &str)]) -> Answer {
        let php = get(&self.php, target, headers).without_cookie_values();
        let plain = get(&self.plain, target, headers).without_cookie_values();
        let warm = get(&self.warm, target, headers).without_cookie_values();
        assert_eq!(plain, php, "{target}: without the warm-up, against PHP");
        assert_eq!(warm, php, "{target}: with the warm-up, against PHP");
        warm
    }
}

#[test]
fn every_route_answers_as_without_the_warm_up_and_as_php() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    let ping = symfony.same_answer("/api/ping?name=ada", &[]);
    assert_eq!(ping.body, r#"{"pong":true,"name":"ada"}"#);
    // A listener of the application is still built inside the request that
    // first needs it, not before the snapshot.
    assert_eq!(ping.header("x-listener-built-for"), ["/api/ping"]);

    let products = symfony.same_answer("/api/products", &[]);
    assert_eq!(
        products.body,
        r#"[[1,"Lamp",1200],[2,"Desk",34000],[3,"Chair",8900]]"#
    );

    // Nothing of the boot is visible: the request is the client's own, no
    // session, no managed entity, no transaction, the time is this request's.
    let state = symfony.same_answer("/api/state", &[("X-Probe", "mine"), ("Cookie", "plain=1")]);
    assert_eq!(
        state.body,
        r#"{"request":{"is_the_main_one":true,"host":"localhost","url":"http:\/\/localhost\/api\/ping","header":"mine","cookies":{"plain":"1"}},"session_started":false,"time":{"request_time_is_this_request":true,"now_is_this_request":true},"entities_managed":0,"transaction_open":false,"output_buffers":0}"#
    );
    symfony.same_answer("/api/missing", &[]);

    // A session route: the first visit finds nothing, the same visitor
    // finds their name again, another visitor does not.
    for server in [&symfony.php, &symfony.plain, &symfony.warm] {
        let first = get(server, "/visit?name=ada", &[]);
        assert_eq!(first.body, r#"{"before":null}"#);
        let again = get(server, "/visit", &[("Cookie", &first.cookies())]);
        assert_eq!(again.body, r#"{"before":"ada"}"#);
        let other = get(server, "/visit", &[]);
        assert_eq!(other.body, r#"{"before":null}"#);
    }
    symfony.same_answer("/visit", &[]);
}

#[test]
fn the_warm_up_builds_no_service_of_the_application() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    // No entity metadata is loaded and no connection is opened before the
    // snapshot: a request finds what it finds without the warm-up.
    let untouched = r#"{"metadata_loaded":[],"connected":false}"#;
    assert_eq!(get(&symfony.warm, "/api/warmed", &[]).body, untouched);
    assert_eq!(get(&symfony.plain, "/api/warmed", &[]).body, untouched);
}

#[test]
fn randomness_is_drawn_in_the_request() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    let values = |body: &str| -> Vec<String> {
        body.trim_matches(['[', ']'])
            .split(',')
            .map(str::to_string)
            .collect()
    };
    let first = values(&get(&symfony.warm, "/api/random", &[]).body);
    let second = values(&get(&symfony.warm, "/api/random", &[]).body);
    assert_eq!(first.len(), 4);
    for (index, (one, other)) in first.iter().zip(&second).enumerate() {
        assert_ne!(one, other, "value {index} is the same in two requests");
    }
}

#[test]
fn a_warm_up_request_leaves_no_session_and_no_entity() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    // The warm-up stores a name in a session and reads every product.
    let (warmed, _log) = symfony.warmed_by("/visit?name=warm /api/products");

    // An anonymous visitor has no session of the warm-up's...
    assert_eq!(get(&warmed, "/visit", &[]).body, r#"{"before":null}"#);
    // ...two visitors have their own...
    let ada = get(&warmed, "/visit?name=ada", &[]).cookies();
    let bob = get(&warmed, "/visit?name=bob", &[]).cookies();
    assert_eq!(
        get(&warmed, "/visit", &[("Cookie", &ada)]).body,
        r#"{"before":"ada"}"#
    );
    assert_eq!(
        get(&warmed, "/visit", &[("Cookie", &bob)]).body,
        r#"{"before":"bob"}"#
    );
    assert_eq!(get(&warmed, "/visit", &[]).body, r#"{"before":null}"#);
    // ...and no entity the warm-up loaded is still managed: Symfony's own
    // reset of its services runs at the start of every request.
    let state = get(&symfony.php, "/api/state", &[]).body;
    assert!(
        state.contains(r#""entities_managed":0,"transaction_open":false"#),
        "{state}"
    );
    assert_eq!(get(&warmed, "/api/state", &[]).body, state);
    assert_eq!(
        get(&warmed, "/api/products", &[]).body,
        get(&symfony.php, "/api/products", &[]).body
    );
}

#[test]
fn a_listener_the_warm_up_built_was_built_for_the_warm_up() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    // The stated cost of a warm-up request: a service it made Symfony
    // build is the one every request gets. This application's listener
    // reads the current request in its constructor; under PHP that is the
    // request being answered, here it is the warm-up's.
    let (warmed, _log) = symfony.warmed_by("/visit");
    assert_eq!(
        get(&symfony.php, "/api/ping", &[]).header("x-listener-built-for"),
        ["/api/ping"]
    );
    assert_eq!(
        get(&symfony.warm, "/api/ping", &[]).header("x-listener-built-for"),
        ["/api/ping"]
    );
    assert_eq!(
        get(&warmed, "/api/ping", &[]).header("x-listener-built-for"),
        ["/visit"]
    );
}

#[test]
fn a_warm_up_request_that_fails_does_not_stop_the_worker() {
    let Some(symfony) = Symfony::start() else {
        return;
    };
    let (warmed, log) = symfony.warmed_by("/fails /api/missing /api/products");
    assert_eq!(
        get(&warmed, "/api/products", &[]).body,
        get(&symfony.php, "/api/products", &[]).body
    );
    let log = std::fs::read_to_string(log).expect("the server's log");
    for (path, status) in [("/fails", 500), ("/api/missing", 404)] {
        let line = format!("the warm-up request {path} was answered with status {status}");
        assert_eq!(log.matches(&line).count(), 1, "{line}\n{log}");
    }
    assert!(!log.contains("the warm-up request /api/products"), "{log}");
    assert!(!log.contains("DEFECT"), "{log}");
}
