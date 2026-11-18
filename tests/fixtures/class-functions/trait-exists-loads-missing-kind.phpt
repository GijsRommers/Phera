--TEST--
Trait exists loads missing kind
--FILE--
<?php
spl_autoload_register(function($name) { echo $name; class Example {} }); var_dump(trait_exists("Example"));
--EXPECT--
Examplebool(false)
