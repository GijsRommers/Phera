--TEST--
Exceptions: sweep_previous_chain
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
function translate_failure(): void
{
    try {
        throw new Exception('socket');
    } catch (Exception $failure) {
        throw new TransportFailure('transport', 1, $failure);
    }
}
try {
    try {
        translate_failure();
    } catch (TransportFailure $failure) {
        throw new ApplicationFailure('checkout', 2, $failure);
    }
} catch (Throwable $failure) {
    while ($failure !== null) {
        echo $failure->getMessage(), '|', $failure->getCode(), "\n";
        $failure = $failure->getPrevious();
    }
}

--EXPECTF--
checkout|2
transport|1
socket|0
