--TEST--
traits: insteadof method
--FILE--
<?php
trait T{function m(){return "T";}} trait U{function m(){return "U";}} class C{use T,U { T::m insteadof U; }} echo (new C)->m();
--EXPECT--
T
