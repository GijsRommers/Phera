--TEST--
A running generator rejects a recursive resume
--FILE--
<?php
function running() {
    global $generator;
    yield 'initial';
    try {
        $generator->next();
    } catch (Error $error) {
        yield $error->getMessage();
    }
}
$generator = running();
var_dump($generator->current());
$generator->next();
var_dump($generator->current());
--EXPECT--
string(7) "initial"
string(42) "Cannot resume an already running generator"
