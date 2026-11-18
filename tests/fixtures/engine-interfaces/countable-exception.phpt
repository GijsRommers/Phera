--TEST--
countable exception
--FILE--
<?php
class Bag implements Countable { public function count(): int { echo "count\n"; return 3; } } $bag=new Bag;
class Broken implements Countable { public function count(): int {throw new Exception("count hook");} } try {count(new Broken);} catch(Exception $e) {echo $e->getMessage(),"\n";}
--EXPECT--
count hook
