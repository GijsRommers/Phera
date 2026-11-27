<?php

// The server side of http_client.rs: a router the client script talks to.

use Phera\Http\{Router, Request, Response};

$router = new Router();
$router
    ->get('/hello', fn () => 'Hello')
    ->any('/echo', fn (Request $r) => Response::json([
        'method' => $r->method(),
        'query' => $r->query(),
        'body' => $r->body(),
        'type' => $r->header('content-type'),
        'tag' => $r->header('x-tag'),
        'agent' => $r->header('user-agent'),
    ], 200, ['X-Server' => 'phera', 'Set-Cookie' => ['a=1', 'b=2']]))
    ->get('/redirect', fn () => Response::empty(302, ['Location' => '/hello']))
    ->post('/see-other', fn () => Response::empty(303, ['Location' => '/echo']))
    ->post('/temporary', fn () => Response::empty(307, ['Location' => '/echo']))
    ->get('/loop', fn () => Response::empty(302, ['Location' => '/loop']))
    ->get('/sleep/{ms}', function (Request $r) {
        usleep((int) $r->param('ms') * 1000);
        return 'slept ' . $r->param('ms');
    })
    ->get('/status/{code}', fn (Request $r) => Response::text('status', (int) $r->param('code')));
return $router;
