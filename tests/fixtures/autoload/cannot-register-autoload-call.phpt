--TEST--
Cannot register autoload call
--FILE--
<?php
try { spl_autoload_register("spl_autoload_call"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
spl_autoload_register(): Argument #1 ($callback) must not be the spl_autoload_call() function
