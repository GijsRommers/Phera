--TEST--
Class members: nullsafe-present
--FILE--
<?php $a=new stdClass; $a->x=3; var_dump($a?->x);
--EXPECT--
int(3)
