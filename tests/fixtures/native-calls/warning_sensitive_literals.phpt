--TEST--
Builtins with diagnostics keep warning until their call executes
--FILE--
<?php
error_reporting(E_ALL);
echo "start\n";
set_error_handler(function ($level, $message) {
    echo $level, ': ', $message, "\n";
    return true;
});
echo "ord=", ord(''), "\n";
echo "chr=", bin2hex(chr(256)), "\n";
echo "strlen=", strlen(null), "\n";
--EXPECT--
start
ord=8192: ord(): Providing an empty string is deprecated
0
chr=8192: chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256
00
strlen=8192: strlen(): Passing null to parameter #1 ($string) of type string is deprecated
0
