--TEST--
capture array snapshot
--FILE--
<?php
$a=[1,2];$f=function()use($a){$a[]=3;return $a;};$a[]=9;var_dump($f(),$f(),$a);
--EXPECT--
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(9)
}
