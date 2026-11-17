--TEST--
Performance cache inherited_scope
--FILE--
<?php
class A {private $x=2;function read($o){echo $o->x,"\n";}}class B extends A {private $x=9;}class C extends A {private $x=8;} $b=new B;$c=new C;$b->read($b);$c->read($c);$b->read($c);
--EXPECT--
2
2
2
