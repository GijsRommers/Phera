--TEST--
Generator methods before the first yield and after returning
--FILE--
<?php
function numbers() {
    echo "begin\n";
    yield 1;
    yield 2;
    return 'done';
}
$generator = numbers();
try {
    $generator->getReturn();
} catch (Exception $error) {
    echo $error->getMessage(), "\n";
}
$generator->rewind();
var_dump($generator->key(), $generator->current());
$generator->next();
try {
    $generator->rewind();
} catch (Exception $error) {
    echo $error->getMessage(), "\n";
}
$generator->next();
var_dump($generator->valid(), $generator->current(), $generator->key(), $generator->getReturn());
try {
    foreach ($generator as $value) {}
} catch (Exception $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
begin
Cannot get return value of a generator that hasn't returned
int(0)
int(1)
Cannot rewind a generator that was already run
bool(false)
NULL
NULL
string(4) "done"
Cannot traverse an already closed generator
