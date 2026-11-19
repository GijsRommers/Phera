--TEST--
Literal sprintf performs argument evaluation before casts and handles escaped percent signs
--FILE--
<?php
$word = 'before';
echo sprintf('%s %s; %d%%', $word, $word = 'after', '5'), "\n";
echo sprintf('%02d', 5), "\n";
set_error_handler(function ($level, $message) { echo "$message\n"; return true; });
function formatTail() { echo "evaluated\n"; return 1; }
echo sprintf('%s %d', [], formatTail()), "\n";
--EXPECT--
after after; 5%
05
evaluated
Array to string conversion
Array 1
