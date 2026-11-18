--TEST--
Get parent class autoload
--FILE--
<?php
class A {} spl_autoload_register(function($name) { class B extends A {} }); var_dump(get_parent_class("B"));
--EXPECT--
string(1) "A"
