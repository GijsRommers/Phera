--TEST--
A thousand levels of parentheses, arrays, calls and unary operators
--FILE--
<?php
echo eval('return ' . str_repeat('(', 1000) . '7' . str_repeat(')', 1000) . ';'), "\n";
$a = eval('return ' . str_repeat('[', 1000) . '1' . str_repeat(']', 1000) . ';');
$depth = 0;
while (is_array($a)) { $a = $a[0]; $depth++; }
echo $depth, "\n";
echo eval('return ' . str_repeat('abs(', 1000) . '-3' . str_repeat(')', 1000) . ';'), "\n";
var_dump(eval('return ' . str_repeat('!', 1001) . 'true;'));
echo eval('return ' . str_repeat('- ', 1000) . '5;'), "\n";
--EXPECT--
7
1000
3
bool(false)
5
