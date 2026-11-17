--TEST--
Exceptions: sweep_hierarchy
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
function load_order(int $number): array
{
    if ($number < 1) {
        throw new ValidationFailure('invalid order');
    }
    return ['number' => $number];
}
foreach ([3, -1, 7] as $number) {
    try {
        $order = load_order($number);
        echo 'order:', $order['number'], "\n";
    } catch (ApplicationFailure $failure) {
        print_response(error_response($failure));
    }
}

--EXPECTF--
order:3
500|ValidationFailure|invalid order
order:7
