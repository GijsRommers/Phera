--TEST--
Generator send validates rejected calls before resuming and accepts named arguments
--FILE--
<?php
function acceptValue() {
    $left = yield 'start';
    $right = yield $left;
    yield $right;
}
$generator = acceptValue();
echo $generator->current(), "\n";
foreach ([[], ['first', 'extra']] as $arguments) {
    try {
        $generator->send(...$arguments);
    } catch (ArgumentCountError $error) {
        echo $error->getMessage(), "\n";
    }
    echo $generator->current(), "\n";
}
echo $generator->send('positional'), "\n";
echo $generator->send(value: 'named'), "\n";
--EXPECT--
start
Generator::send() expects exactly 1 argument, 0 given
start
Generator::send() expects exactly 1 argument, 2 given
start
positional
named
