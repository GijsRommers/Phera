--TEST--
Is a string first argument with and without allow_string
--FILE--
<?php
class Base {}
class Child extends Base {}
var_dump(is_a('Child', 'Base', true));
var_dump(is_a('child', 'BASE', true));
var_dump(is_a('Child', 'Base'));
var_dump(is_a('Child', 'Base', false));
var_dump(is_a('Nope', 'Base', true));
spl_autoload_register(function ($name) { echo "autoload $name\n"; });
var_dump(is_a('Nope', 'Base', true));
var_dump(is_a('Nope', 'Base'));
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
autoload Nope
bool(false)
bool(false)
