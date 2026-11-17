--TEST--
Call window: outer receiver reassignment
--FILE--
<?php
class A{function f($a){echo 'A';}} class B{function f($a){echo 'B';}} $x=new A; $x->f($x=new B);$x->f(0);
--EXPECT--
AB
