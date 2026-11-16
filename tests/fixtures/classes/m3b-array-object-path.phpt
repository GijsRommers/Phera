--TEST--
Class members: array-object-path
--FILE--
<?php $array=[new stdClass]; $array[0]->x=1; var_dump($array[0]->x);
--EXPECT--
int(1)
