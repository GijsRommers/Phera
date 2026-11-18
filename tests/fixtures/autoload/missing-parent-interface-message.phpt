--TEST--
Missing parent interface message
--FILE--
<?php
spl_autoload_register(fn($name) => null); try { class Child implements MissingI {} } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Interface "MissingI" not found
