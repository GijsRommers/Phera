--TEST--
Register invalid function
--FILE--
<?php
try { spl_autoload_register("missing_function"); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
spl_autoload_register(): Argument #1 ($callback) must be a valid callback or null, function "missing_function" not found or invalid function name
