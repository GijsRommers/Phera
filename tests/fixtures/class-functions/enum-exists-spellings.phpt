--TEST--
Enum exists spellings
--FILE--
<?php
enum Suit: string { case Hearts = 'h'; }
enum Pure { case A; }
class Base {}
var_dump(enum_exists('Suit'));
var_dump(enum_exists('suit'));
var_dump(enum_exists('Pure'));
var_dump(enum_exists('Base'));
var_dump(enum_exists('Nope'));
var_dump(enum_exists('Nope', false));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
