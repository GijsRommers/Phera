--TEST--
Unregister unregistered function
--FILE--
<?php
function actual($class) {} var_dump(spl_autoload_unregister("actual"));
--EXPECT--
bool(false)
