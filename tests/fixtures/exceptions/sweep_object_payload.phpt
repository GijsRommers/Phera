--TEST--
Exceptions: sweep_object_payload
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
function read_payload(string $input): object
{
    return json_decode($input, false, 512, JSON_THROW_ON_ERROR);
}
foreach (['{"account":{"number":1}}', '{'] as $input) {
    try {
        $payload = read_payload($input);
        echo get_class($payload), '|';
        echo $payload->account->number, "\n";
    } catch (JsonException $failure) {
        echo 'rejected:', $failure->getCode(), "\n";
    }
}
echo 'finished';

--EXPECTF--
stdClass|1
rejected:4
finished