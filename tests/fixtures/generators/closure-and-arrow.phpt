--TEST--
Generators retain captured closure references and arrow values
--FILE--
<?php
$count = 1;
$ref = function () use (&$count) {
    yield $count++;
    yield $count++;
};
$arrow = fn() => yield $count;
$first = $ref();
$second = $arrow();
$count = 5;
var_dump($first->current());
$first->next();
var_dump($first->current(), $second->current(), $count);
--EXPECT--
int(5)
int(6)
int(1)
int(7)
