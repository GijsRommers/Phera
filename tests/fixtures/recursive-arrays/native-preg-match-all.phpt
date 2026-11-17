--TEST--
Self-referential array passed to preg-match-all
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(preg_match_all("/a/", "aa", $a[5]), $a);
--EXPECT--
int(2)
array(1) {
  [0]=>
  array(2) {
    [0]=>
    string(1) "a"
    [1]=>
    string(1) "a"
  }
}
