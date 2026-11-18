--TEST--
traits: abstract method
--FILE--
<?php
trait T{abstract function m():int;function run(){return $this->m();}} class C{use T;function m():int{return 5;}} echo (new C)->run();
--EXPECT--
5
