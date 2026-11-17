--TEST--
Exceptions: sweep_retry
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
function send_request(int $attempt, bool $unavailable): string
{
    if ($unavailable || $attempt < 3) {
        throw new TransportFailure('temporary');
    }
    return 'delivered';
}
foreach ([false, true] as $unavailable) {
    $attempts = 0;
    try {
        while (true) {
            try {
                $attempts++;
                echo send_request($attempts, $unavailable), '|', $attempts;
                break;
            } catch (TransportFailure $failure) {
                echo 'retry:', $attempts, "\n";
                if ($attempts >= 3) {
                    throw $failure;
                }
            }
        }
    } catch (ApplicationFailure $failure) {
        echo 'exhausted:', $attempts, '|', $failure->getMessage(), "\n";
    }
}

--EXPECTF--
retry:1
retry:2
delivered|3retry:1
retry:2
retry:3
exhausted:3|temporary
