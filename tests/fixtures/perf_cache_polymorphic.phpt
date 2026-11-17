--TEST--
Performance cache polymorphic
--FILE--
<?php
class A { public $x=1; function get(){return $this->x;} } class B extends A { public $x=2; function get(){return $this->x+10;} } function show($o){echo $o->get(), ':', $o->x, "\n";} show(new A);show(new B);show(new A);
--EXPECT--
1:1
12:2
1:1
