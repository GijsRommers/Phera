--TEST--
countable plain object
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
try {count(new stdClass);} catch(TypeError $e) {echo $e->getMessage(),"\n";}
--EXPECT--
count(): Argument #1 ($value) must be of type Countable|array, stdClass given
