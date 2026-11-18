--TEST--
array_keys uses the filter value as the left operand
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1, 5 => []];
try { var_dump(array_keys([$b], $a)); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
