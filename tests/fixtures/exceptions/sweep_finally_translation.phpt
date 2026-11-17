--TEST--
Exceptions: sweep_finally_translation
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
function execute_operation(): void
{
    try {
        throw new TransportFailure('network');
    } finally {
        throw new ApplicationFailure('cleanup');
    }
}
try {
    execute_operation();
} catch (ApplicationFailure $failure) {
    print_response(error_response($failure));
    $previous = $failure->getPrevious();
    echo get_class($previous), '|';
    echo $previous->getMessage(), "\n";
}

--EXPECTF--
500|ApplicationFailure|cleanup
TransportFailure|network
