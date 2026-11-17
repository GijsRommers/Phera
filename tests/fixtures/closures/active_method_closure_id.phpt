--TEST--
active method closure id
--FILE--
<?php
class A{function g($x){return spl_object_id(new stdClass);}}$a=new A;$f=$a->g(...);var_dump($f($f=null));
--EXPECT--
int(3)
