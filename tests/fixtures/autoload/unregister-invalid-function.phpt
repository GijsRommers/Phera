--TEST--
Unregister invalid function
--FILE--
<?php
try { spl_autoload_unregister("missing_function"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
spl_autoload_unregister(): Argument #1 ($callback) must be a valid callback, function "missing_function" not found or invalid function name
