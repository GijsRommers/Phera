--TEST--
Call window: spread sparse integer keys
--FILE--
<?php
function f(...$rest){var_dump($rest);}f(...[9=>'a',2=>'b',40=>'c']);
--EXPECT--
array(3) {
  [0]=>
  string(1) "a"
  [1]=>
  string(1) "b"
  [2]=>
  string(1) "c"
}
