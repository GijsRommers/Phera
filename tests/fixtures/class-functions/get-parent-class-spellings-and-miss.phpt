--TEST--
Get parent class spellings and miss
--FILE--
<?php
class Base {}
class Child extends Base {}
class_alias('Base', 'AliasOfBase');
$base = new Base; $child = new Child; $std = new stdClass;
var_dump(get_parent_class($child));
var_dump(get_parent_class($base));
var_dump(get_parent_class($std));
var_dump(get_parent_class('Child'));
var_dump(get_parent_class('child'));
var_dump(get_parent_class('\\Child'));
var_dump(get_parent_class('AliasOfBase'));
try { get_parent_class('Nope'); } catch (Throwable $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
--EXPECT--
string(4) "Base"
bool(false)
bool(false)
string(4) "Base"
string(4) "Base"
string(4) "Base"
bool(false)
TypeError: get_parent_class(): Argument #1 ($object_or_class) must be an object or a valid class name, string given
