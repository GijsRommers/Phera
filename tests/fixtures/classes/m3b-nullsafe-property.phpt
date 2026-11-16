--TEST--
Class members: nullsafe-property
--FILE--
<?php $a=null; var_dump($a?->x);
--EXPECT--
NULL
