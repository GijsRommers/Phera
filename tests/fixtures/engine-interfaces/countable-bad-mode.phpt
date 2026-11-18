--TEST--
countable bad mode
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
try {count($bag, 7);} catch(ValueError $e) {echo $e->getMessage(),"\n";}
--EXPECT--
count(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE
