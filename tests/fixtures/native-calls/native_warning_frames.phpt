--TEST--
Exact typed native calls and specialized instructions expose different warning frames
--FILE--
<?php
error_reporting(E_ALL);
set_error_handler(function ($level, $message) {
    echo $message, "\n";
    $trace = debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS);
    echo $trace[1]['function'] ?? 'no native frame', "\n";
    return true;
});
chr(256);
strlen(null);
strlen(string: null);
array_key_exists(null, []);
--EXPECT--
chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256
chr
strlen(): Passing null to parameter #1 ($string) of type string is deprecated
no native frame
strlen(): Passing null to parameter #1 ($string) of type string is deprecated
strlen
Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead
no native frame
