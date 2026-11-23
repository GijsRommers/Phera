--TEST--
Empty skips a bare getter on the final property
--FILE--
<?php
class Getter { function __get($p) { echo "get\n"; return 7; } }
var_dump(empty((new Getter)->x));
--EXPECT--
bool(true)
