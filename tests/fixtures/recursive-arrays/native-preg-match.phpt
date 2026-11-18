--TEST--
Self-referential array passed to preg-match
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(preg_match("/a/", "a", $a[5]), $a);
--EXPECT--
int(1)
array(1) {
  [0]=>
  string(1) "a"
}
