<?php

use Phera\Grpc\Client;
use Phera\Grpc\Status;
use Phera\Grpc\StatusException;

$schema = require __DIR__ . '/schema.php';
[$endpoint, $tls] = [$argv[count($argv) - 2], $argv[count($argv) - 1]];
$options = $tls === '-' ? [] : ['caFile' => "$tls/cert.pem"];

function show(string $label, mixed $value): void
{
    echo $label, ': ', json_encode($value), "\n";
}

function attempt(string $label, callable $call): void
{
    try {
        show($label, $call());
    } catch (StatusException $e) {
        echo $label, ': ', Status::name($e->getCode()), ' ', json_encode($e->grpcMessage), ' ', json_encode($e->metadata), "\n";
    }
}

$c = new Client($endpoint, $schema, $options);
show('raw', $c->unary('/echo.Echo/Raw', 'abc'));
show('say', $c->unary('/echo.Echo/Say', ['message' => 'hi', 'count' => 3], metadata: ['x-user' => 'alice']));
show('say headers', $c->lastHeaders());
$trailers = $c->lastTrailers();
show('say trailers', [$trailers['x-length'], bin2hex($trailers['trace-bin'])]);
show('count', $c->serverStream('/echo.Echo/Count', ['message' => 'n', 'count' => 3], 'echo.Req', 'echo.Resp'));
show('list', $c->serverStream('/echo.Echo/List', ['count' => 0], 'echo.Req', 'echo.Resp'));
attempt('broken', fn () => $c->serverStream('/echo.Echo/Broken', [], 'echo.Req', 'echo.Resp'));
show('sum', $c->clientStream('/echo.Echo/Sum', [['count' => 1], ['count' => 2], ['count' => 39]], 'echo.Req', 'echo.Sum'));
attempt('fail', fn () => $c->unary('/echo.Echo/Fail', ['message' => 'x'], 'echo.Req', 'echo.Resp'));
attempt('crash', fn () => $c->unary('/echo.Echo/Crash', [], 'echo.Req', 'echo.Resp'));
attempt('unknown', fn () => $c->unary('/echo.Echo/Nope', ''));
attempt('slow', fn () => $c->unary('/echo.Echo/Slow', [], 'echo.Req', 'echo.Resp', timeoutMs: 100));
show('meta', json_decode($c->unary('/echo.Echo/Meta', '', metadata: ['x-a' => '1', 'x-b' => ['2', '3']], timeoutMs: 5000)));

// Compressed both ways: a gzip request is answered with gzip.
$gzip = new Client($endpoint, $schema, $options + ['compression' => 'gzip']);
$long = str_repeat('z', 5000);
show('gzip', $gzip->unary('/echo.Echo/Say', ['message' => $long, 'count' => 1])['message'] === $long);

show('health', bin2hex($c->unary('/grpc.health.v1.Health/Check', '')));
show('health service', bin2hex($c->unary('/grpc.health.v1.Health/Check', "\x0a\x09echo.Echo")));
attempt('health unknown', fn () => $c->unary('/grpc.health.v1.Health/Check', "\x0a\x03abc"));

attempt('bad request', function () use ($c) {
    try {
        return $c->unary('/echo.Echo/Say', ['nope' => 1]);
    } catch (Phera\Protobuf\Exception $e) {
        return $e->getMessage();
    }
});
show('stop', $c->unary('/echo.Echo/Stop', ''));
