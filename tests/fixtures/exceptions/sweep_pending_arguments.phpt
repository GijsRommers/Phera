--TEST--
Exceptions: sweep_pending_arguments
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
function decode_payload(string $payload): array
{
    return json_decode($payload, true, 512, JSON_THROW_ON_ERROR);
}
function store_record(string $key, array $record): void
{
    echo $key, '|', $record['number'], "\n";
}
foreach (['{"number":1}', '{', '{"number":3}'] as $payload) {
    try {
        store_record('record', decode_payload($payload));
    } catch (JsonException $failure) {
        echo 'invalid:', $failure->getCode(), "\n";
    }
}
echo 'done';

--EXPECTF--
record|1
invalid:4
record|3
done