--TEST--
constant nonstatic closure error
--FILE--
<?php
const C=function(){};
--EXPECTF--

Fatal error: Closures in constant expressions must be static in %s on line 2
