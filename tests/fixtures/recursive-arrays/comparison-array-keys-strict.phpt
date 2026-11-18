--TEST--
array-keys-strict throws on distinct recursive arrays
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
try { var_dump(array_keys(['item' => $b], $a, true)); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
