--TEST--
Generators that delegate to the same object share its current value
--FILE--
<?php
function numbers() {
    yield 1;
    yield 2;
    yield 3;
    return 4;
}
function proxy($generator) {
    return yield from $generator;
}
$input = numbers();
$first = proxy($input);
$second = proxy($input);
var_dump($first->current(), $second->current());
$first->next();
var_dump($second->current());
$second->next();
var_dump($first->current());
$first->next();
var_dump($first->getReturn(), $second->current());
$second->next();
var_dump($second->getReturn());
--EXPECT--
int(1)
int(1)
int(2)
int(3)
int(4)
int(3)
int(4)
