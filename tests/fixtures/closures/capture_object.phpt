--TEST--
capture object
--FILE--
<?php
class Box{public $x=1;}$a=new Box;$f=function()use($a){return ++$a->x;};var_dump($f(),$a->x);
--EXPECT--
int(2)
int(2)
