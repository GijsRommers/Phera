--TEST--
gap-named-variadic
--FILE--
<?php function f(...$args){var_dump($args);}f(key:1);
--EXPECT--
array(1) {
  ["key"]=>
  int(1)
}
