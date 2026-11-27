<?php

// A router whose client is made at boot, before the snapshot: every
// forked request uses it, and its connection stays open between requests.

use Phera\Http\{Client, Router, Request};

$upstream = new Client(['base_uri' => getenv('UPSTREAM')]);
$router = new Router();
$router->get('/proxy/{n}', fn (Request $r) => $upstream->get('/n/' . $r->param('n'))->body());
return $router;
