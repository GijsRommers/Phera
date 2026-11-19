--TEST--
A named limit bounds the number of frames
--FILE--
<?php
function outer() { inner(); }
function inner() {
    echo count(debug_backtrace(limit: 1)), " ", count(debug_backtrace(0, -1)), "\n";
}
outer();
--EXPECT--
1 0
