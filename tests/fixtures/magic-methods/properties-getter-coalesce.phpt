--TEST--
Coalesce calls __get without an isset hook
--FILE--
<?php
class Getter { function __get($p) { echo "get\n"; return 7; } }
echo (new Getter)->x ?? 3;
--EXPECT--
get
7
