<?php
// The built-in HTTP server, Phera\Http\Server, on the three routes of the
// usual web framework benchmarks.
//
//     PORT=8080 WORKERS=4 phera benchmarks/bench-phera-http.php
//     oha -z 10s -c 64 http://127.0.0.1:8080/plaintext
//     oha -z 10s -c 64 http://127.0.0.1:8080/json
//     oha -z 10s -c 64 http://127.0.0.1:8080/users/42
//
// PORT defaults to 8080 (0 picks a free port; the line printed at start
// says which), WORKERS to the number of CPUs. Stop it with Ctrl-C.

use Phera\Http\{Request, Response, Server};

$port = getenv('PORT');
$workers = getenv('WORKERS');
$server = new Server(
    '127.0.0.1:' . ($port === false ? '8080' : $port),
    workers: $workers === false ? 0 : (int) $workers,
);

$server
    ->get('/plaintext', fn() => 'Hello, World!')
    ->get('/json', fn() => Response::json(['message' => 'Hello, World!']))
    ->get('/users/{id}', fn(Request $request) => Response::json([
        'id' => (int) $request->param('id'),
        'name' => 'User ' . $request->param('id'),
    ]));

echo "listening on http://", $server->address(), " with ", $server->workers(), " workers\n";
$server->listen();
