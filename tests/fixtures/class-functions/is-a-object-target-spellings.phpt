--TEST--
Is a object with target spellings
--FILE--
<?php
interface Marker {}
interface SubMarker extends Marker {}
class Base implements Marker {}
class Child extends Base implements SubMarker {}
enum Suit: string { case Hearts = 'h'; }
$child = new Child;
var_dump(is_a($child, 'Marker'));
var_dump(is_a($child, '\\Marker'));
var_dump(is_a($child, 'marker'));
var_dump(is_a($child, 'SubMarker'));
var_dump(is_a($child, 'Base'));
var_dump(is_a($child, 'Child'));
var_dump(is_a($child, 'Suit'));
var_dump(is_a($child, 'Nope'));
var_dump(is_a($child, 'Base', false));
var_dump(is_a(Suit::Hearts, 'Suit'));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(true)
bool(true)
