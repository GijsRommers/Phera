--TEST--
traits: trait composition
--FILE--
<?php
trait T { function m(){return 9;} } trait U {use T;} trait V {use U;} class C {use V;} echo (new C)->m();
--EXPECT--
9
