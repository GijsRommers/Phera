--TEST--
Self-referential array passed to walk-recursive
--FILE--
<?php
$a = [1];
$a[5] = &$a;
try { array_walk_recursive($a, function ($value) { var_dump($value); }); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
int(1)
int(1)
Error: Recursion detected
done
