--TEST--
An explicit buffer flush frames the PHP output callback
--FILE--
<?php
$trace = null;
ob_start(function ($text) use (&$trace) {
    $trace = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    return $text;
});
echo "buffer\n";
ob_end_flush();
foreach ($trace as $frame) {
    echo $frame['function'], ' ', $frame['line'] ?? 'internal', "\n";
}
--EXPECTF--
buffer
{closure:%s/output-callback.php:3} internal
ob_end_flush 8
