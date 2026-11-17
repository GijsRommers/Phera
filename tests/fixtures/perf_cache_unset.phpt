--TEST--
Property cache handles unset and reinsertion
--FILE--
<?php
class A {public int $x=2;public $y=9;}
function read($a){echo isset($a->x) ? $a->x : 'uninitialized', "\n";}
$a=new A;read($a);unset($a->x);read($a);$a->x=7;read($a);
--EXPECT--
2
uninitialized
7
