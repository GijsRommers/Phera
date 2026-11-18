--TEST--
traits: nested trait identity
--FILE--
<?php
trait T{function m(){return __TRAIT__;}} trait U{use T;} class C{use U;} echo (new C)->m();
--EXPECT--
T
