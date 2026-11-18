--TEST--
countable interface type
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
function take(Countable $it): Countable {return $it;} var_dump(take($bag)===$bag);
--EXPECT--
bool(true)
