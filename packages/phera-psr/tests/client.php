<?php

declare(strict_types=1);

// PSR-18 against tests/app.php, which serves PSR-15 through the router:
// `phera tests/client.php http://127.0.0.1:PORT`.

require __DIR__ . '/bootstrap.php';

use Phera\Http\Client;
use Phera\Psr\HttpFactory;
use Phera\Psr\NetworkException;
use Phera\Psr\Psr18Client;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Client\NetworkExceptionInterface;

$base = $argv[1] ?? throw new \RuntimeException('usage: client.php http://host:port');
$factory = new HttpFactory();
$client = new Psr18Client();
Check::true($client instanceof ClientInterface, 'Psr18Client is a ClientInterface');

$request = $factory->createRequest('POST', "$base/psr/thing?page=2&tags[]=a")
    ->withHeader('X-Tag', 'tagged')
    ->withHeader('Content-Type', 'application/x-www-form-urlencoded')
    ->withHeader('Cookie', 'session=abc; theme=dark')
    ->withBody($factory->createStream('name=Ada&lang=php'));
$response = $client->sendRequest($request);
Check::same(200, $response->getStatusCode(), 'PSR-18 status');
Check::same('OK', $response->getReasonPhrase(), 'reason phrase');
Check::same('application/json', $response->getHeaderLine('content-type'), 'content type');
Check::same('timing', $response->getHeaderLine('X-Middleware'), 'PSR-15 middleware ran');
Check::same(['1', '2'], $response->getHeader('x-multi'), 'header lines');
$seen = json_decode((string) $response->getBody(), true, flags: JSON_THROW_ON_ERROR);
Check::same([
    'method' => 'POST',
    'path' => '/psr/thing',
    // PSR-7 percent-encodes the brackets; PHP's query parsing reads them back.
    'target' => '/psr/thing?page=2&tags%5B%5D=a',
    'query' => ['page' => '2', 'tags' => ['a']],
    'attributes' => ['rest' => 'thing', 'timing' => 'on'],
    'parsed' => ['name' => 'Ada', 'lang' => 'php'],
    'cookies' => ['session' => 'abc', 'theme' => 'dark'],
    'tag' => 'tagged',
    'body' => 'name=Ada&lang=php',
], $seen, 'the PSR-15 handler saw the PSR-7 request');

$blocked = $client->sendRequest($factory->createRequest('GET', "$base/psr/x")->withHeader('X-Block', 'yes'));
// Guard runs inside Timing: it answers without the handler, and Timing
// still wraps its response.
Check::same([403, 'blocked', 'timing'], [$blocked->getStatusCode(), (string) $blocked->getBody(), $blocked->getHeaderLine('X-Middleware')], 'inner middleware short-circuits');

$redirect = $client->sendRequest($factory->createRequest('GET', "$base/redirect"));
Check::same([302, '/psr/elsewhere'], [$redirect->getStatusCode(), $redirect->getHeaderLine('Location')], 'PSR-18 returns redirects as they are');
$following = new Psr18Client(new Client(['max_redirects' => 3]));
$followed = $following->sendRequest($factory->createRequest('GET', "$base/redirect"));
Check::same([200, '/psr/elsewhere'], [$followed->getStatusCode(), json_decode((string) $followed->getBody(), true)['path']], 'a client that follows redirects');

$missing = $client->sendRequest($factory->createRequest('GET', "$base/nowhere"));
Check::same(404, $missing->getStatusCode(), 'an error status is a response');

$offline = $factory->createRequest('GET', 'http://127.0.0.1:1/');
try {
    $client->sendRequest($offline);
    Check::true(false, 'connection refused throws');
} catch (NetworkExceptionInterface $exception) {
    Check::true($exception instanceof NetworkException, 'NetworkException');
    Check::true($exception->getRequest() === $offline, 'the exception keeps the request');
}

Check::report('phera-psr http');
