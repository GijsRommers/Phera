--TEST--
Nested handlers route deprecations outward and preserve replacement output when the error handler throws
--FILE--
<?php
function outer_handler($bytes) {
    return '[' . $bytes . ']';
}
function inner_handler($bytes) {
    echo 'discarded';
    return strtoupper($bytes);
}

ob_start('outer_handler');
ob_start('inner_handler');
echo 'inner';
ob_end_flush();
ob_end_flush();
echo "\n";

set_error_handler(function ($type, $message) {
    throw new Exception("$type: $message");
});
ob_start('inner_handler');
echo 'done';
try {
    ob_end_flush();
} catch (Exception $exception) {
    echo "\ncaught ", $exception->getMessage(), "\n";
}
--EXPECTF--

Deprecated: ob_end_flush(): Producing output from user output handler outer_handler is deprecated in %s on line 14
[
Deprecated: ob_end_flush(): Producing output from user output handler inner_handler is deprecated in %s on line 13
INNER]
DONE
caught 8192: ob_end_flush(): Producing output from user output handler inner_handler is deprecated
