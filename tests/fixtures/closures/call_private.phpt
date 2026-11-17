--TEST--
call private
--FILE--
<?php
class Box{private $x=4;}$f=function($a){return $this->x+$a;};var_dump($f->call(new Box,3));
--EXPECT--
int(7)
