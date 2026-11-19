--TEST--
Only explicit integer keys advance a generator's automatic key
--FILE--
<?php
function keys() {
    yield 'start';
    yield -2 => 'negative';
    yield 10 => 'ten';
    yield '15' => 'string';
    yield null => 'null';
    yield 'end';
}
foreach (keys() as $key => $value) {
    var_dump($key, $value);
}
--EXPECT--
int(0)
string(5) "start"
int(-2)
string(8) "negative"
int(10)
string(3) "ten"
string(2) "15"
string(6) "string"
NULL
string(4) "null"
int(11)
string(3) "end"
