--TEST--
traits: namespace resolution
--FILE--
<?php
namespace Ns; trait T {function m(){return 5;}} class C{use T;} echo (new C)->m();
--EXPECT--
5
