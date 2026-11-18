--TEST--
countable recursive
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
var_dump(count($bag, COUNT_RECURSIVE));
--EXPECT--
count
int(3)
