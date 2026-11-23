--TEST--
Long chains on the right of an assignment
--FILE--
<?php
// PHP folds constant strings while parsing, so this one compiles at any length.
eval('$s = ' . str_repeat("'ab' . ", 200000) . "'c';");
echo strlen($s), "\n";
// PHP's compiler recurses on other chains; 30,000 terms fit its stack.
eval('$x = ' . str_repeat('1 + ', 30000) . '1;');
echo $x, "\n";
$a = 2;
eval('$y = $a' . str_repeat(' * $a - $a', 15000) . ';');
echo $y, "\n";
--EXPECT--
400001
30001
-59994
