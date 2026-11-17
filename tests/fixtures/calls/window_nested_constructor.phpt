--TEST--
Call window: nested constructor
--FILE--
<?php
class A{function __construct(public $x){echo "A:$x\n";}} function f($a,$b){echo $a->x+$b->x;} f(new A(2),new A(3));
--EXPECT--
A:2
A:3
5
