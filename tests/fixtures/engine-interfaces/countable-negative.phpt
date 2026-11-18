--TEST--
countable negative
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
class Odd implements Countable { public function count(): int { return -3; } } var_dump(count(new Odd));
--EXPECT--
int(-3)
