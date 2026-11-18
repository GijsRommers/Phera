--TEST--
traits: diamond composition
--FILE--
<?php
trait T {function m(){return 1;}} trait U{use T;} trait V{use T;} class C{use U,V;} echo (new C)->m();
--EXPECT--
1
