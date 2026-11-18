--TEST--
countable size
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
var_dump(sizeof($bag));
--EXPECT--
count
int(3)
