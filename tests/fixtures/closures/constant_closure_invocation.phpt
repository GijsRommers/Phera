--TEST--
constant closure invocation
--FILE--
<?php
const C=static function($x){return $x+2;};var_dump((C)(3));
--EXPECT--
int(5)
