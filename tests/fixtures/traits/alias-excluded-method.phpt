--TEST--
traits: alias excluded method
--FILE--
<?php
trait T{function m(){return "T";}} trait U{function m(){return "U";}} class C{use T,U { T::m insteadof U; U::m as n; }} echo (new C)->n();
--EXPECT--
U
