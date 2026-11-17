--TEST--
Class members: nullsafe-method
--FILE--
<?php $a=null; var_dump($a?->method(print "bad")->x);
--EXPECT--
NULL
