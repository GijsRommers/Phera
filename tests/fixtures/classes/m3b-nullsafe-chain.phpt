--TEST--
Class members: nullsafe-chain
--FILE--
<?php $a=null; var_dump($a?->x->y["k"]);
--EXPECT--
NULL
