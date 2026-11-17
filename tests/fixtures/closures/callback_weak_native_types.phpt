--TEST--
callback weak native types
--FILE--
<?php
declare(strict_types=1);var_dump(array_map(strlen(...),[3,44]));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
