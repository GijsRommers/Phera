--TEST--
Exceptions: sweep_warning_conversion
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
function warning_handler($errno, $message, $file, $line): void
{
    throw new ErrorException($message, 0, $errno, $file, $line);
}
set_error_handler('warning_handler');
$warnings = 0;
foreach (['first', 'second'] as $message) {
    try {
        trigger_error($message, E_USER_WARNING);
    } catch (ErrorException $failure) {
        echo $failure->getMessage(), '|', $failure->getSeverity(), "\n";
        $warnings++;
    } finally {
        echo 'finished', "\n";
    }
}
restore_error_handler();
echo $warnings;

--EXPECTF--
first|512
finished
second|512
finished
2