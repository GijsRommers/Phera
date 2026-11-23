--TEST--
Trait exists spellings
--FILE--
<?php
trait Helper {}
class Base { use Helper; }
var_dump(trait_exists('Helper'));
var_dump(trait_exists('helper'));
var_dump(trait_exists('\\Helper'));
var_dump(trait_exists('Base'));
var_dump(trait_exists('Nope'));
var_dump(trait_exists('Nope', false));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
