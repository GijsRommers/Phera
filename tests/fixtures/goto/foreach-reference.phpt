--TEST--
A jump out of reference foreach preserves the last PHP reference
--FILE--
<?php
$a = [1, 2];
foreach ($a as &$v) {
    $a[] = 3;
    goto done;
}
done:
$v = 4;
unset($v);
var_dump($a);
--EXPECT--
array(3) {
  [0]=>
  int(4)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
