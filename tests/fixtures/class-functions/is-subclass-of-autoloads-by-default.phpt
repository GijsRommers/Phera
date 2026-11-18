--TEST--
Is subclass of autoloads by default
--FILE--
<?php
spl_autoload_register(function($name) { echo $name; class Example extends stdClass {} }); var_dump(is_subclass_of("Example","stdClass"));
--EXPECT--
Examplebool(true)
