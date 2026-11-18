--TEST--
rsort finishes renumbering after a recursive comparison error
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
$values = ['z' => $a, 'b' => $b, 'a' => 2];
try { rsort($values); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
var_dump(array_keys($values), key($values));
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
array(3) {
  [0]=>
  int(0)
  [1]=>
  int(1)
  [2]=>
  int(2)
}
int(0)
