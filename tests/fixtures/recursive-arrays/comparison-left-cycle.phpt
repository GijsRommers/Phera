--TEST--
Only a repeated left table triggers the recursion error
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1, 5 => []];
try { var_dump($a <=> $b); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
var_dump($b <=> $a);
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
int(-1)
