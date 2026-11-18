--TEST--
The overloaded property notice precedes a missing local source warning
--FILE--
<?php
class Items { function __get($name) { return null; } }
$items = new Items;
$items->entries[0]['k'] = $missing;
--EXPECTF--
Notice: Indirect modification of overloaded property Items::$entries has no effect in %s on line 4

Warning: Undefined variable $missing in %s on line 4
