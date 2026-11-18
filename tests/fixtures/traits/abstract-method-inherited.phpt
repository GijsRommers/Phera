--TEST--
traits: abstract method inherited
--FILE--
<?php
trait T{abstract function m():int;} class P{function m():int{return 1;}} class C extends P{use T;} echo (new C)->m();
--EXPECT--
1
