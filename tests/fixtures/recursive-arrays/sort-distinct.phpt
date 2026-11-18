--TEST--
sort throws and still renumbers the input array
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
$values = ['a' => $a, 'b' => $b];
try { sort($values); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
var_dump(array_keys($values));
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
array(2) {
  [0]=>
  int(0)
  [1]=>
  int(1)
}
