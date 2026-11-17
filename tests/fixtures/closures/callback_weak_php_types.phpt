--TEST--
callback weak php types
--FILE--
<?php
declare(strict_types=1);$f=fn(int $x)=>$x+1;var_dump(array_map($f,["3","4"]));
--EXPECT--
array(2) {
  [0]=>
  int(4)
  [1]=>
  int(5)
}
