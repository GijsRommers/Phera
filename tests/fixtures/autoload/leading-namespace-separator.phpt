--TEST--
Leading namespace separator
--FILE--
<?php
spl_autoload_register(fn($name) => print($name)); class_exists("\\App\\Wanted");
--EXPECT--
App\Wanted