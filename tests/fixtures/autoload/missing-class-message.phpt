--TEST--
Missing class message
--FILE--
<?php
spl_autoload_register(fn($name) => null); try { new Unknown; } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Class "Unknown" not found
