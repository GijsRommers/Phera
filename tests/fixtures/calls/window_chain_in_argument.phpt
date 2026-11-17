--TEST--
Call window: chain in argument
--FILE--
<?php
class A{function next(){echo 'n';return $this;} function value($x){echo 'v';return $x+1;}}function sum($a,$b){return $a+$b;}$a=new A;echo sum(2,$a->next()->next()->value(4));
--EXPECT--
nnv7
