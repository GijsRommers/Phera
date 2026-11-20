--TEST--
A string offset assignment evaluates the index expression before the source
--FILE--
<?php
$string = 'ab';
$string[$index = 0] = $string[$index];
echo $string, ' ', $index, "\n";
--EXPECT--
ab 0
