--TEST--
traits: private abstract method
--FILE--
<?php
trait T{abstract private function m();function run(){return $this->m();}} class C{use T;private function m(){return 5;}} echo (new C)->run();
--EXPECT--
5
