--TEST--
Class exists kinds and spellings
--FILE--
<?php
interface Marker {}
trait Helper {}
abstract class Abs {}
enum Suit: string { case Hearts = 'h'; }
class Base implements Marker { use Helper; }
class_alias('Base', 'AliasOfBase');
var_dump(class_exists('Base'));
var_dump(class_exists('base'));
var_dump(class_exists('\\Base'));
var_dump(class_exists('AliasOfBase'));
var_dump(class_exists('Abs'));
var_dump(class_exists('Suit'));
var_dump(class_exists('stdClass'));
var_dump(class_exists('Marker'));
var_dump(class_exists('Helper'));
var_dump(class_exists('Nope'));
var_dump(class_exists('Nope', false));
var_dump(class_exists('Base', false));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
bool(false)
bool(true)
