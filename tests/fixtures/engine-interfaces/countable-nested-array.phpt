--TEST--
countable nested array
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
var_dump(count([$bag, [1,2]], COUNT_RECURSIVE));
--EXPECT--
int(4)
