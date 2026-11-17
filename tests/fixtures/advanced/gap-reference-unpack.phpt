--TEST--
gap-reference-unpack
--FILE--
<?php function f(&$x){$x=7;}$a=[1];f(...$a);echo $a[0];
--EXPECT--
7