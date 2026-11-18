--TEST--
asort still sorts the input after a recursive comparison fails
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
$values = ['z' => $a, 'b' => $b, 'a' => 2];
next($values);
try { asort($values); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
var_dump(array_keys($values), key($values));
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
array(3) {
  [0]=>
  string(1) "b"
  [1]=>
  string(1) "a"
  [2]=>
  string(1) "z"
}
string(1) "b"
