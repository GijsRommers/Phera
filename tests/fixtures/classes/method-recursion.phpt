--TEST--
Classes: method-recursion
--FILE--
<?php class Counter { function down($number) { if ($number == 0) return 7; return $this->down($number-1); } } echo (new Counter)->down(50);
--EXPECT--
7