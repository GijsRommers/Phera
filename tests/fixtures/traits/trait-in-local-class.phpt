--TEST--
traits: trait in local class
--FILE--
<?php
trait T{function m(){return 2;}} function make(){class C{use T;} return new C;} echo make()->m();
--EXPECT--
2
