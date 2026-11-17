--TEST--
Class members: error-read-null
--FILE--
<?php $a=null; var_dump($a->x);
--EXPECTF--

Warning: Attempt to read property "x" on null in %s on line 1
NULL
