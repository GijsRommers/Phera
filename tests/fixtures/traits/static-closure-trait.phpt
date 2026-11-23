--TEST--
traits: static closure trait
--FILE--
<?php
trait T {function m(){return static function(){return __TRAIT__;};}} class A{use T;} echo ((new A)->m())();
--EXPECT--
T
