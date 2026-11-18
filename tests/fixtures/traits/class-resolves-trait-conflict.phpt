--TEST--
traits: class resolves trait conflict
--FILE--
<?php
trait T {function m(){}} trait U {function m(){}} class C {use T,U; function m(){return 4;}} echo (new C)->m();
--EXPECT--
4
