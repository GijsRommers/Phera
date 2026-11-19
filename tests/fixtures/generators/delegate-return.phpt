--TEST--
Yield from forwards keys and returns the inner generator's return value
--FILE--
<?php
function source() {
    $reply = yield 'question' => 1;
    yield $reply;
    return 12;
}
function target() {
    yield from [0 => 'array', 7 => 'seven'];
    $result = yield from source();
    yield $result;
    return 20;
}
$generator = target();
var_dump($generator->key(), $generator->current());
$generator->next();
var_dump($generator->key(), $generator->current());
$generator->next();
var_dump($generator->key(), $generator->current());
var_dump($generator->send('answer'), $generator->key());
$generator->next();
var_dump($generator->key(), $generator->current());
$generator->next();
var_dump($generator->getReturn());
--EXPECT--
int(0)
string(5) "array"
int(7)
string(5) "seven"
string(8) "question"
int(1)
string(6) "answer"
int(0)
int(0)
int(12)
int(20)
