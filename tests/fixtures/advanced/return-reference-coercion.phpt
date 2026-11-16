--TEST--
return-reference-coercion
--FILE--
<?php
function &number(): float { static $value = 1; return $value; }
$alias =& number(); $alias = 2; var_dump(number(), $alias);
function &arrayReference(): array { static $value = [1]; return $value; }
$items =& arrayReference(); $items[] = 2; var_dump(arrayReference());

--EXPECT--
float(2)
float(2)
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
