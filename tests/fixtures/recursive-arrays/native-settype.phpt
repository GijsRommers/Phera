--TEST--
Self-referential array passed to settype
--FILE--
<?php
$a = [1];
$a[5] = &$a;
var_dump(settype($a[5], "bool"), $a);
--EXPECT--
bool(true)
bool(true)
