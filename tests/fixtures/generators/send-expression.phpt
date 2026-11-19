--TEST--
Send resumes yield expressions in nested pending calls
--FILE--
<?php
function show($prefix, $value, $suffix) {
    echo "$prefix$value$suffix\n";
}
function receive() {
    show('=', (yield 'first' => 1) + (yield 2), '!');
    $last = yield;
    return $last;
}
$generator = receive();
var_dump($generator->send(20));
var_dump($generator->send(22));
var_dump($generator->send('done'));
var_dump($generator->getReturn());
--EXPECT--
int(2)
=42!
NULL
NULL
string(4) "done"
