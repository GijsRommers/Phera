--TEST--
Exceptions: sweep_router
--FILE--
<?php
class ApplicationFailure extends RuntimeException {}
class ValidationFailure extends ApplicationFailure {}
class TransportFailure extends ApplicationFailure {}
function error_response(Throwable $failure): array
{
    return [
        'status' => 500,
        'error' => get_class($failure),
        'message' => $failure->getMessage(),
    ];
}
function print_response(array $response): void
{
    echo $response['status'], '|';
    echo $response['error'], '|';
    echo $response['message'], "\n";
}
function dispatch_route(string $route): string
{
    if ($route == '/health') {
        return 'healthy';
    }
    if ($route == '/broken') {
        return undefined_service();
    }
    throw new ApplicationFailure('not found');
}
foreach (['/health', '/broken', '/missing'] as $route) {
    try {
        echo dispatch_route($route), "\n";
    } catch (Throwable $failure) {
        print_response(error_response($failure));
    }
}

--EXPECTF--
healthy
500|Error|Call to undefined function undefined_service()
500|ApplicationFailure|not found
