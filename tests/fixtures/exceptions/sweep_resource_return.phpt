--TEST--
Exceptions: sweep_resource_return
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
function read_resource(bool $empty): string
{
    echo 'open|';
    try {
        if ($empty) {
            return 'empty';
        }
        return 'content';
    } finally {
        echo 'close|';
    }
}
foreach ([false, true] as $empty) {
    $result = read_resource($empty);
    echo $result, "\n";
}

--EXPECTF--
open|close|content
open|close|empty
