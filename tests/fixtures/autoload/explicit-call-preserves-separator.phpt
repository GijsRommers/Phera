--TEST--
Explicit call preserves separator
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); spl_autoload_call("\\App\\Wanted");
--EXPECT--
\App\Wanted