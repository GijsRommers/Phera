--TEST--
Exceptions: sweep_rethrow
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
function require_configuration(): void
{
    throw new ValidationFailure('missing secret');
}
$observed = null;
try {
    try {
        require_configuration();
    } catch (ValidationFailure $failure) {
        $observed = $failure;
        echo 'logged', "\n";
        throw $failure;
    }
} catch (ApplicationFailure $failure) {
    var_dump($observed === $failure);
    print_response(error_response($failure));
}

--EXPECTF--
logged
bool(true)
500|ValidationFailure|missing secret
