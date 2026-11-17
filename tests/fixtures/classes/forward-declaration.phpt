--TEST--
Classes: forward-declaration
--FILE--
<?php var_dump(new Example instanceof Example); class Example {}
--EXPECT--
bool(true)
