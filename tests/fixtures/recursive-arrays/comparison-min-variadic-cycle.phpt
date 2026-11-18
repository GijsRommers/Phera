--TEST--
Variadic min compares the next argument on the left
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1, 5 => []];
try { var_dump(min($b, $a, 0)); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
