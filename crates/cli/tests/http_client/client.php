<?php

// The client side of http_client.rs: `phera client.php http://127.0.0.1:PORT`.

use Phera\Http\{Client, ClientException, Response};

$client = new Client(['base_uri' => $argv[1], 'headers' => ['X-Tag' => 'default']]);
$show = fn (string $label, mixed $value) => print($label . ': ' . json_encode($value, JSON_UNESCAPED_SLASHES) . "\n");

$hello = $client->get('/hello');
$show('get', [$hello instanceof Response, $hello->status(), $hello->body(), $hello->header('content-type'), $hello->header('x-none', 'fallback')]);

$echo = $client->post('/echo?dropped=1', [
    'json' => ['name' => 'Ada'],
    'query' => ['q' => 'a b', 'list' => [1, 2]],
    'headers' => ['X-Tag' => 'override'],
]);
$show('post json', $echo->decodeJson());
$show('response headers', [$echo->header('x-server'), $echo->headers()['set-cookie'], $echo->header('set-cookie')]);
$show('put body', $client->put('/echo', ['body' => 'raw', 'headers' => ['Content-Type' => 'text/plain']])->decodeJson());
$show('patch', $client->patch('/echo', ['body' => 'p'])->decodeJson()['method']);
$show('delete', $client->delete('/echo')->decodeJson()['method']);
$show('request', $client->request('options', '/echo')->decodeJson()['method']);
$head = $client->head('/hello');
$show('head', [$head->status(), $head->body()]);
$show('decode object', $client->get('/echo')->decodeJson(false)->method);

$show('redirect', [$client->get('/redirect')->status(), $client->get('/redirect')->body()]);
$show('303 becomes GET', $client->post('/see-other', ['body' => 'x'])->decodeJson()['method']);
$temporary = $client->post('/temporary', ['body' => 'kept'])->decodeJson();
$show('307 repeats', [$temporary['method'], $temporary['body']]);
$manual = new Client(['base_uri' => $argv[1], 'max_redirects' => 0]);
$show('redirects off', [$manual->get('/redirect')->status(), $manual->get('/redirect')->header('location')]);
$show('per request', $manual->get('/redirect', ['max_redirects' => 2])->body());

try {
    $client->get('/loop');
} catch (ClientException $e) {
    $show('loop', [$e::class, $e->getMessage()]);
}
try {
    $client->get('/sleep/2000', ['timeout' => 0.2]);
} catch (ClientException $e) {
    $show('timeout', $e->getMessage());
}
$show('error status', [$client->get('/status/404')->status(), $client->get('/status/503')->body()]);

$started = hrtime(true);
$answers = $client->pool([
    'first' => ['uri' => '/sleep/300'],
    'second' => ['method' => 'GET', 'uri' => '/sleep/300'],
    7 => ['uri' => '/echo', 'method' => 'POST', 'json' => [1, 2]],
]);
$elapsed = (hrtime(true) - $started) / 1e9;
$show('pool', array_map(fn (Response $r) => $r->body(), $answers));
$show('pool ran at once', $elapsed < 0.55);

try {
    (new Client(['connect_timeout' => 2]))->get('http://127.0.0.1:1/');
} catch (ClientException $e) {
    $show('refused', str_contains($e->getMessage(), 'Connection refused'));
}
try {
    new Client(['timeuot' => 1]);
} catch (ValueError $e) {
    $show('typo', $e->getMessage());
}
try {
    $client->get('/echo', ['body' => 'a', 'json' => 'b']);
} catch (ValueError $e) {
    $show('body and json', $e->getMessage());
}
$show('exception class', is_subclass_of(ClientException::class, RuntimeException::class));
