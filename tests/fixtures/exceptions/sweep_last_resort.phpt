--TEST--
Exceptions: sweep_last_resort
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
function last_resort(Throwable $failure): void
{
    print_response(error_response($failure));
}
function process_job(): void
{
    try {
        throw new ApplicationFailure('job failed');
    } finally {
        echo 'released job', "\n";
    }
}
set_exception_handler('last_resort');
process_job();
echo 'unreachable';

--EXPECTF--
released job
500|ApplicationFailure|job failed
