--TEST--
Exceptions: sweep_continue_cleanup
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
$completed = 0;
foreach ([1, 2, 3, 4] as $number) {
    try {
        if ($number == 2) continue;
        if ($number == 4) break;
        echo 'process:', $number, "\n";
        $completed++;
    } finally {
        echo 'release:', $number, "\n";
    }
}
echo 'completed:', $completed;

--EXPECTF--
process:1
release:1
release:2
process:3
release:3
release:4
completed:2