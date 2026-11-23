--TEST--
Is subclass of object and string first argument
--FILE--
<?php
interface Marker {}
class Base implements Marker {}
class Child extends Base {}
$base = new Base; $child = new Child;
var_dump(is_subclass_of($child, 'Base'));
var_dump(is_subclass_of($child, 'Marker'));
var_dump(is_subclass_of($child, 'Child'));
var_dump(is_subclass_of($base, 'Base'));
var_dump(is_subclass_of($child, 'Nope'));
var_dump(is_subclass_of('Child', 'Base'));
var_dump(is_subclass_of('Child', 'Base', true));
var_dump(is_subclass_of('Child', 'Child', true));
var_dump(is_subclass_of('Nope', 'Base', true));
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
bool(true)
bool(true)
bool(false)
bool(false)
