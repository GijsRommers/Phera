--TEST--
Exceptions: sweep_constructor
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
class Checkout
{
    public int $number;
    public function __construct(int $number)
    {
        if ($number < 1) throw new ValidationFailure('invalid checkout');
        $this->number = $number;
    }
}
foreach ([1, -1, 2] as $number) {
    try {
        $checkout = new Checkout($number);
        echo $checkout->number, "\n";
    } catch (ApplicationFailure $failure) {
        print_response(error_response($failure));
    }
}

--EXPECTF--
1
500|ValidationFailure|invalid checkout
2
