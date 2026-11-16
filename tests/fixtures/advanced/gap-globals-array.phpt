--TEST--
globals-array
--FILE--
<?php $GLOBALS['x']=3;echo $x;
--EXPECT--
3