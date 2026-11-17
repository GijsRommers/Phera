--TEST--
extract
--FILE--
<?php
extract(['a'=>1,'b'=>2]);echo $a,$b;
--EXPECT--
12