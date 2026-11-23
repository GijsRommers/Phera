--TEST--
Interface exists spellings
--FILE--
<?php
interface Marker {}
interface SubMarker extends Marker {}
class Base implements Marker {}
var_dump(interface_exists('Marker'));
var_dump(interface_exists('SubMarker'));
var_dump(interface_exists('\\Marker'));
var_dump(interface_exists('Base'));
var_dump(interface_exists('Nope'));
var_dump(interface_exists('Nope', false));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
