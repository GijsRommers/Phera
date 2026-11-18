--TEST--
traits: static closure state
--FILE--
<?php
trait T {static function m(){return static function(){static $n=0;return ++$n;};}} class A{use T;} class B{use T;} $a=A::m();$b=B::m();echo $a(),$b(),$a(),$b();
--EXPECT--
1122
