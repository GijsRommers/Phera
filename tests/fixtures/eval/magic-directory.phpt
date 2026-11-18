--TEST--
Magic directory
--FILE--
<?php
var_dump(eval('return __DIR__;') === __DIR__);
--EXPECT--
bool(true)
