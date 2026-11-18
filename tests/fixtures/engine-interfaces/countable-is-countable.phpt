--TEST--
countable is countable
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
var_dump(is_countable($bag), is_countable(new stdClass), is_countable([]));
--EXPECT--
bool(true)
bool(false)
bool(true)
