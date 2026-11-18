--TEST--
Unregister default case insensitive
--FILE--
<?php
spl_autoload_register(); var_dump(spl_autoload_unregister("SPL_AUTOLOAD"), spl_autoload_functions());
--EXPECT--
bool(true)
array(0) {
}
