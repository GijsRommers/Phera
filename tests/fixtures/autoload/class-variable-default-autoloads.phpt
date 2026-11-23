--TEST--
Class variable default autoloads
--FILE--
<?php
class A { public $p = Dependency::Y; }
spl_autoload_register(function($name) { echo $name, "\n"; class Dependency { const Y=7; } });
var_dump(get_class_vars(A::class));
--EXPECT--
Dependency
array(1) {
  ["p"]=>
  int(7)
}
