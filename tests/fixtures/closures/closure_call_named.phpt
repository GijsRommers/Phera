--TEST--
closure call named
--FILE--
<?php
class A{private $x=3;}$f=function($a=2,$b=4){return $this->x+$a*$b;};var_dump($f->call(new A,b:5,a:7));
--EXPECT--
int(38)
