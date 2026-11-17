--TEST--
static reference capture
--FILE--
<?php
$x=0;$f=function()use(&$x){static $n=10;return [$x++,++$n];};var_dump($f(),$f());
--EXPECT--
array(2) {
  [0]=>
  int(0)
  [1]=>
  int(11)
}
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(12)
}
