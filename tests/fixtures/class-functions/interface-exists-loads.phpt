--TEST--
Interface exists loads
--FILE--
<?php
spl_autoload_register(function($name) { echo $name; interface Example {} }); var_dump(interface_exists("Example"));
--EXPECT--
Examplebool(true)
