--TEST--
Exceptions: sweep_native_coercion
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
function coerce_input(string $operation): void
{
    if ($operation == 'intdiv') intdiv([], 2);
    if ($operation == 'repeat') str_repeat('a', -1);
    if ($operation == 'combine') array_combine([1], [2, 3]);
    if ($operation == 'json') json_decode('{', false, 512, JSON_THROW_ON_ERROR);
}
foreach (['intdiv', 'repeat', 'combine', 'json'] as $operation) {
    try {
        coerce_input($operation);
    } catch (TypeError | ValueError $failure) {
        echo $operation, '|', get_class($failure), "\n";
    } catch (JsonException $failure) {
        echo $operation, '|', get_class($failure), '|', $failure->getCode(), "\n";
    }
}

--EXPECTF--
intdiv|TypeError
repeat|ValueError
combine|ValueError
json|JsonException|4
