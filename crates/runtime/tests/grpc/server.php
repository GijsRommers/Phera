<?php

use Phera\Grpc\Context;
use Phera\Grpc\Server;
use Phera\Grpc\Status;
use Phera\Grpc\StatusException;

$schema = require __DIR__ . '/schema.php';
[$address, $tls] = [$argv[count($argv) - 2], $argv[count($argv) - 1]];
$options = ['handleSignals' => false, 'shutdownTimeoutMs' => 2000];
if ($tls !== '-') {
    $options['tls'] = ['cert' => "$tls/cert.pem", 'key' => "$tls/key.pem"];
}

$server = new Server($address, $schema, $options);
$server
    // No message types: the handler sees and returns bytes.
    ->unary('/echo.Echo/Raw', fn (string $bytes, Context $ctx) => strrev($bytes))
    ->unary('/echo.Echo/Say', function (array $req, Context $ctx) {
        $ctx->setHeader('x-handler', 'say');
        $ctx->setTrailer('x-length', (string) strlen($req['message']));
        $ctx->setTrailer('trace-bin', "\x00\x01\x02");
        return ['message' => $req['message'], 'index' => $req['count'], 'user' => $ctx->header('x-user') ?? ''];
    })
    ->serverStream('/echo.Echo/Count', function (array $req, Context $ctx) {
        for ($i = 1; $i <= $req['count']; $i++) {
            yield ['message' => $req['message'], 'index' => $i];
        }
    }, 'echo.Req', 'echo.Resp')
    ->serverStream('/echo.Echo/List', fn (array $req) => [['index' => 1], ['index' => 2]], 'echo.Req', 'echo.Resp')
    ->serverStream('/echo.Echo/Broken', function (array $req) {
        yield ['index' => 1];
        throw new StatusException(Status::ABORTED, 'stream broke');
    }, 'echo.Req', 'echo.Resp')
    ->clientStream('/echo.Echo/Sum', function (array $requests, Context $ctx) {
        return ['total' => array_sum(array_column($requests, 'count'))];
    }, 'echo.Req', 'echo.Sum')
    ->unary('/echo.Echo/Fail', function (array $req) {
        throw new StatusException(Status::NOT_FOUND, "no echo for {$req['message']}: 100% gone", ['x-reason' => 'missing']);
    }, 'echo.Req', 'echo.Resp')
    ->unary('/echo.Echo/Crash', function () {
        throw new RuntimeException('boom');
    }, 'echo.Req', 'echo.Resp')
    ->unary('/echo.Echo/Slow', function (array $req, Context $ctx) {
        usleep(300_000);
        return ['message' => 'late'];
    }, 'echo.Req', 'echo.Resp')
    ->unary('/echo.Echo/Meta', function (string $req, Context $ctx) {
        $metadata = $ctx->metadata();
        ksort($metadata);
        return json_encode([$ctx->method(), $metadata, $ctx->deadlineMs() > 0, $ctx->header('grpc-timeout') !== null]);
    })
    ->unary('/echo.Echo/Stop', function (string $req) use ($server) {
        $server->stop();
        return 'bye';
    })
    ->enableHealthService();

echo "listening\n";
$server->listen();
echo "stopped\n";
