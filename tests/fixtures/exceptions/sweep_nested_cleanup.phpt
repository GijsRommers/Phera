--TEST--
Exceptions: sweep_nested_cleanup
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
function transaction(bool $fail): int
{
    try {
        try {
            if ($fail) throw new TransportFailure('rollback');
            return 7;
        } finally {
            echo 'unlock', "\n";
        }
    } finally {
        echo 'release connection', "\n";
    }
}
foreach ([false, true] as $fail) {
    try { echo transaction($fail), "\n"; }
    catch (ApplicationFailure $failure) { echo $failure->getMessage(), "\n"; }
}

--EXPECTF--
unlock
release connection
7
unlock
release connection
rollback
