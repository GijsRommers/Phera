--TEST--
traits: namespace import
--FILE--
<?php
namespace A {trait T {function m(){return 5;}}} namespace B {use A\T as U; class C{use U;} echo (new C)->m();}
--EXPECT--
5
