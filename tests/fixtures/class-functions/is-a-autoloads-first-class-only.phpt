--TEST--
Is a autoloads first class only
--FILE--
<?php
spl_autoload_register(function($name) { echo $name; class Example extends stdClass {} }); var_dump(is_a("Example","stdClass",true));
--EXPECT--
Examplebool(true)
