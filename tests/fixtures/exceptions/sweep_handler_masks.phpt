--TEST--
Exceptions: sweep_handler_masks
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
function notice_handler($errno, $message): bool
{
    echo 'notice:', $message, "\n";
    return true;
}
set_error_handler('notice_handler', E_USER_NOTICE);
$previous = error_reporting(0);
foreach (['start', 'finish'] as $message) {
    trigger_error($message, E_USER_NOTICE);
    trigger_error('hidden', E_USER_WARNING);
}
restore_error_handler();
error_reporting($previous);
echo 'restored', "\n";

--EXPECTF--
notice:start
notice:finish
restored
