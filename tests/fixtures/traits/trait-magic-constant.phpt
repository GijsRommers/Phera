--TEST--
traits: trait magic constant
--FILE--
<?php
namespace Ns; trait T{static function m(){return __TRAIT__;}} class C{use T;} echo C::m();
--EXPECT--
Ns\T
