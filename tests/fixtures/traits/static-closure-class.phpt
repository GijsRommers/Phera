--TEST--
traits: static closure class
--FILE--
<?php
trait T {function m(){return static function(){return __CLASS__;};}} class A{use T;} class B{use T;} echo ((new A)->m())(), ((new B)->m())();
--EXPECT--
AB
