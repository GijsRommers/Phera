--TEST--
Exceptions: sweep_arithmetic
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
function share_total(int $total, int $people): int
{
    return intdiv($total, $people);
}
$completed = 0;
foreach ([2, 0, 5] as $people) {
    try {
        echo share_total(100, $people), "\n";
        $completed++;
    } catch (DivisionByZeroError $failure) {
        echo 'cannot share:', $failure->getMessage(), "\n";
    } finally {
        echo 'accounted', "\n";
    }
}
echo $completed;

--EXPECTF--
50
accounted
cannot share:Division by zero
accounted
20
accounted
2