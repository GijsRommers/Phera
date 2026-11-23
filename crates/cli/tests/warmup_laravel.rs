//! Laravel under `phera serve` with the warm-up before the snapshot.
//!
//! ```php
//! // What Phera's Laravel entry runs once per worker:
//! $app->make(Kernel::class)->bootstrap();
//! Phera\Laravel\WarmUp::beforeSnapshot($app);   // models booted, services resolved
//! Phera\snapshot();
//! $app->handleRequest(Request::capture());      // every request, in its own copy
//! ```
//!
//! The warm-up may change what a request costs and nothing else. Every test
//! asks the same application three ways: `phera serve`,
//! `phera serve --no-service-warmup` (as `PHERA_SERVICE_WARMUP=0`), and PHP's
//! own server running Laravel's own `public/index.php`.
//!
//! The application is `tests/warmup_laravel/`; its packages come from an
//! installed Laravel application named by `PHERA_LARAVEL_APP` (see
//! `framework_support`). Without it the tests are skipped.

mod framework_support;

use std::path::Path;
use std::process::Command;

use framework_support::{Answer, Project, get, installed_application};
use phera_extension_api::testing::{oracle, serve::Server};

/// The application, and the three servers answering for it.
struct Laravel {
    warm: Server,
    plain: Server,
    php: Server,
    // Dropped last: the servers run in it.
    project: Project,
}

impl Laravel {
    fn start() -> Option<Laravel> {
        let installed = installed_application("PHERA_LARAVEL_APP")?;
        let php = oracle::find_php().expect("PHP 8.5 oracle");
        let skeleton = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/warmup_laravel");
        let project = Project::new(&skeleton, &installed);
        let database = project.root.join("database/test.sqlite");
        project.write(
            ".env",
            &format!(
                "APP_ENV=production\nAPP_DEBUG=false\nAPP_URL=http://warmup.test\n\
                 APP_KEY=base64:{KEY}\nDB_CONNECTION=sqlite\nDB_DATABASE={}\n\
                 SESSION_DRIVER=file\nCACHE_STORE=file\nLOG_CHANNEL=single\n",
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

        let phera = Path::new(env!("CARGO_BIN_EXE_phera"));
        let root = project.document_root();
        Some(Laravel {
            warm: Server::start(phera, &root, 1, &[]),
            plain: Server::start(phera, &root, 1, &[("PHERA_SERVICE_WARMUP", "0")]),
            php: Server::start_php(&php, &root, 1),
            project,
        })
    }

    /// One more server, which handled `paths` as requests before its
    /// snapshot (`PHERA_WARMUP`, what `--warmup` sets), and the file its
    /// log is written to.
    fn warmed_by(&self, paths: &str) -> (Server, std::path::PathBuf) {
        let log = self.project.root.join("storage/logs/server.log");
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

/// 32 bytes, base64: an application key for the tests only.
const KEY: &str = "cGhlcmEtd2FybS11cC10ZXN0LWtleS0wMTIzNDU2Nzg=";

#[test]
fn every_route_answers_as_without_the_warm_up_and_as_php() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let posts = laravel.same_answer("/api/posts?page=2", &[]);
    assert_eq!(posts.status, 200);
    // The pagination links carry the request's host (`Host: localhost`), not
    // APP_URL's (`warmup.test`).
    assert!(
        posts
            .body
            .contains(r#""next_page_url":"http:\/\/localhost\/api\/posts?page=3""#),
        "{}",
        posts.body
    );

    // Nothing of the boot is visible: the request is the client's own, no
    // user, no session, no transaction, the time is this request's.
    let state = laravel.same_answer("/api/state", &[("X-Probe", "mine"), ("Cookie", "plain=1")]);
    assert_eq!(state.status, 200);
    assert!(
        state.body.contains(r#""is_the_bound_one":true"#),
        "{}",
        state.body
    );
    assert!(
        state.body.contains(r#""user":null,"signed_in":false"#),
        "{}",
        state.body
    );
    assert!(
        state
            .body
            .contains(r#""laravel_start_is_this_request":true,"request_time_is_this_request":true,"carbon_now_is_this_request":true,"carbon_has_test_time":false"#),
        "{}",
        state.body
    );
    assert!(
        state.body.contains(r#""transaction_level":0"#),
        "{}",
        state.body
    );

    // A session route: the first visit finds nothing, the same visitor
    // finds their name again, another visitor does not.
    for server in [&laravel.php, &laravel.plain, &laravel.warm] {
        let first = get(server, "/visit?name=ada", &[]);
        assert_eq!(first.body, r#"{"before":null,"token_length":40}"#);
        let again = get(server, "/visit", &[("Cookie", &first.cookies())]);
        assert_eq!(again.body, r#"{"before":"ada","token_length":40}"#);
        let other = get(server, "/visit", &[]);
        assert_eq!(other.body, r#"{"before":null,"token_length":40}"#);
    }
    laravel.same_answer("/visit", &[]);
    laravel.same_answer("/api/missing", &[]);
    drop(laravel.project);
}

#[test]
fn the_warm_up_boots_the_models_and_leaves_nothing_of_a_request() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    // Post, Stamp and User are booted before the snapshot. Note is not (its boot
    // asks for the signed-in user): it was tried once and put back, with
    // its listener removed. No connection, session store or guard is left.
    assert_eq!(
        get(&laravel.warm, "/api/warmed", &[]).body,
        r#"{"booted_models":["App\\Models\\Post","App\\Models\\Stamp","App\\Models\\User"],"note_boot_attempts":1,"note_listeners":0,"connections":{"sqlite":"not opened"},"session_stores":0,"guards":[],"paginator_declared":false}"#
    );
    assert_eq!(
        get(&laravel.plain, "/api/warmed", &[]).body,
        r#"{"booted_models":[],"note_boot_attempts":0,"note_listeners":0,"connections":[],"session_stores":0,"guards":[],"paginator_declared":false}"#
    );

    // The model that needs a request boots in the request, once, with both
    // of its scopes and one listener, as under PHP.
    let notes = laravel.same_answer("/api/notes", &[]);
    assert_eq!(
        notes.body,
        r#"{"sql":"select * from \"notes\" where \"visible\" = ? and \"owner_id\" is null","creating_listeners":1}"#
    );
}

/// The stated difference from PHP: a model's `boot()` and `booted()` run
/// once per worker, so what they keep of that moment (here the clock) is
/// the same in every request. Without the warm-up, and under PHP, the model
/// boots in each request that uses it.
#[test]
fn a_model_that_boots_without_the_request_boots_once_per_worker() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let booted_at = |server: &Server| get(server, "/api/stamp", &[]).body;
    assert_eq!(booted_at(&laravel.warm), booted_at(&laravel.warm));
    assert_ne!(booted_at(&laravel.plain), booted_at(&laravel.plain));
    assert_ne!(booted_at(&laravel.php), booted_at(&laravel.php));
}

#[test]
fn time_and_randomness_are_drawn_in_the_request() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let values = |body: &str| -> Vec<String> {
        body.trim_matches(['[', ']'])
            .split(',')
            .map(str::to_string)
            .collect()
    };
    let first = values(&get(&laravel.warm, "/api/random", &[]).body);
    let second = values(&get(&laravel.warm, "/api/random", &[]).body);
    assert_eq!(first.len(), 5);
    for (index, (one, other)) in first.iter().zip(&second).enumerate() {
        assert_ne!(one, other, "value {index} is the same in two requests");
    }
}

#[test]
fn a_worker_that_boots_again_warms_up_again_to_the_same_state() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let before = get(&laravel.warm, "/api/warmed", &[]).body;
    assert!(before.contains(r#""paginator_declared":false"#), "{before}");

    // After 64 requests that autoload nothing new, the worker boots again
    // and declares the classes its requests loaded (the class warm-up).
    let mut after = String::new();
    for _ in 0..400 {
        get(&laravel.warm, "/api/posts", &[]);
        after = get(&laravel.warm, "/api/warmed", &[]).body;
        if after.contains(r#""paginator_declared":true"#) {
            break;
        }
    }
    // That second boot ran the warm-up again, with the same result.
    assert_eq!(
        after,
        before.replace(
            r#""paginator_declared":false"#,
            r#""paginator_declared":true"#
        )
    );
    laravel.same_answer("/api/posts?page=2", &[]);

    // A changed file boots the worker once more: the new route answers,
    // and the state is the same again.
    let routes = laravel.project.root.join("routes/api.php");
    let source = std::fs::read_to_string(&routes).expect("read the routes");
    std::fs::write(
        &routes,
        format!("{source}\nRoute::get('/added', fn () => ['added' => true]);\n"),
    )
    .expect("write the routes");
    let mut added = 0;
    for _ in 0..200 {
        added = get(&laravel.warm, "/api/added", &[]).status;
        if added == 200 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert_eq!(added, 200, "the changed routes file was not picked up");
    assert_eq!(get(&laravel.warm, "/api/warmed", &[]).body, after);
}

/// What an anonymous visitor is to the application, on every server.
const NOBODY: &str = r#"{"user":null,"name":null,"flash":null,"locale":"en","translator":"en","carbon":"en","queued_cookies":[],"log_context":[]}"#;

#[test]
fn a_warm_up_request_leaves_no_session_user_cookie_locale_or_flash() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    // The warm-up signs a user in, fills and flashes the session, queues a
    // cookie, switches the locale, adds log context, and asks for work
    // "after the response". Then a second request stores a name.
    let (warmed, _log) = laravel.warmed_by("/leave-traces /visit?name=warm");

    // An anonymous visitor is nobody, with the cookies PHP would set and
    // no other.
    let nobody = get(&laravel.php, "/whoami", &[]).without_cookie_values();
    assert_eq!(nobody.body, NOBODY);
    assert_eq!(get(&warmed, "/whoami", &[]).without_cookie_values(), nobody);
    assert_eq!(
        get(&warmed, "/visit", &[]).body,
        r#"{"before":null,"token_length":40}"#
    );

    // Two users sign in: each is themselves, and neither is the warm-up.
    let ada = get(&warmed, "/login/1", &[]).cookies();
    let bob = get(&warmed, "/login/2", &[]).cookies();
    assert_eq!(
        get(&warmed, "/whoami", &[("Cookie", &ada)]).body,
        NOBODY.replace(r#""user":null,"name":null"#, r#""user":1,"name":"user 1""#)
    );
    assert_eq!(
        get(&warmed, "/whoami", &[("Cookie", &bob)]).body,
        NOBODY.replace(r#""user":null,"name":null"#, r#""user":2,"name":"user 2""#)
    );
    assert_eq!(get(&warmed, "/whoami", &[]).body, NOBODY);

    // What the warm-up asked to run when its request ended ran once, in
    // the worker's boot, and in none of the requests above.
    let ended = laravel.project.root.join("storage/logs/terminated.log");
    assert_eq!(
        std::fs::read_to_string(ended).unwrap_or_default(),
        "ended\n"
    );
}

#[test]
fn a_warm_up_through_an_authenticated_route_is_reported_and_harmless() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    // `/account` is behind the `auth` middleware; `/login/1` signs a user
    // in without being asked for anything.
    let (warmed, log) = laravel.warmed_by("/account /login/1");
    assert_eq!(get(&warmed, "/whoami", &[]).body, NOBODY);
    // The warm-up carries no credentials: it was sent to the login form.
    let log = std::fs::read_to_string(log).expect("the server's log");
    assert_eq!(
        log.matches("the warm-up request /account was answered with status 302")
            .count(),
        1,
        "{log}"
    );
    // And the route is as closed to an anonymous visitor as on PHP.
    let account = get(&laravel.php, "/account", &[]).without_cookie_values();
    assert_eq!(account.status, 302);
    assert_eq!(
        get(&warmed, "/account", &[]).without_cookie_values(),
        account
    );
}

#[test]
fn a_warm_up_request_that_fails_does_not_stop_the_worker() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let (warmed, log) =
        laravel.warmed_by("/fails /moved /api/missing /fails-in-transaction /api/posts");
    let posts = get(&laravel.php, "/api/posts?page=2", &[]);
    assert_eq!(get(&warmed, "/api/posts?page=2", &[]), posts);
    // The transaction a failed warm-up request left open is gone.
    let state = get(&warmed, "/api/state", &[]);
    assert!(
        state.body.contains(r#""transaction_level":0"#),
        "{}",
        state.body
    );

    let log = std::fs::read_to_string(log).expect("the server's log");
    for (path, status) in [
        ("/fails", 500),
        ("/moved", 302),
        ("/api/missing", 404),
        ("/fails-in-transaction", 500),
    ] {
        let line = format!("the warm-up request {path} was answered with status {status}");
        assert_eq!(log.matches(&line).count(), 1, "{line}\n{log}");
    }
    assert!(!log.contains("the warm-up request /api/posts"), "{log}");
    // The worker has its snapshot: no request ran from the top.
    assert!(!log.contains("DEFECT"), "{log}");
}

#[test]
fn a_connection_the_warm_up_opened_is_each_requests_own() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let (warmed, log) = laravel.warmed_by("/api/posts");
    // The snapshot holds the connection the warm-up used...
    let warmed_state = get(&warmed, "/api/warmed", &[]).body;
    assert!(
        warmed_state.contains(r#""connections":{"sqlite":"opened"}"#),
        "{warmed_state}"
    );
    // ...and every request, also twenty at once, reads through one of its
    // own and sends the statements it would send on PHP.
    let posts = get(&laravel.php, "/api/posts?page=2", &[]).body;
    let answers = warmed.get_all((0..20).map(|_| "/api/posts?page=2".to_string()));
    for answer in answers {
        assert_eq!(
            Answer::parse(&answer)
                .replacing(&warmed.address, "SERVER")
                .body,
            posts
        );
    }
    let statements = get(&laravel.php, "/api/post-statements", &[]).body;
    assert_eq!(statements, r#"{"statements":2}"#);
    assert_eq!(get(&warmed, "/api/post-statements", &[]).body, statements);
    let log = std::fs::read_to_string(log).expect("the server's log");
    assert!(!log.contains("DEFECT"), "{log}");
}

#[test]
fn a_worker_that_boots_again_handles_its_warm_up_requests_again() {
    let Some(laravel) = Laravel::start() else {
        return;
    };
    let (warmed, _log) = laravel.warmed_by("/leave-traces");
    assert_eq!(get(&warmed, "/whoami", &[]).body, NOBODY);
    let ended = laravel.project.root.join("storage/logs/terminated.log");
    assert_eq!(
        std::fs::read_to_string(&ended).unwrap_or_default(),
        "ended\n"
    );

    // A changed file boots the worker again: the new route answers, the
    // warm-up request has run once more, and a visitor is still nobody.
    let routes = laravel.project.root.join("routes/api.php");
    let source = std::fs::read_to_string(&routes).expect("read the routes");
    std::fs::write(
        &routes,
        format!("{source}\nRoute::get('/added', fn () => ['added' => true]);\n"),
    )
    .expect("write the routes");
    let mut added = 0;
    for _ in 0..200 {
        added = get(&warmed, "/api/added", &[]).status;
        if added == 200 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert_eq!(added, 200, "the changed routes file was not picked up");
    assert_eq!(
        std::fs::read_to_string(&ended).unwrap_or_default(),
        "ended\nended\n"
    );
    assert_eq!(get(&warmed, "/whoami", &[]).body, NOBODY);
}
