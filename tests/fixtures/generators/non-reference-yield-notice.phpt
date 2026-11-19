--TEST--
Reference generators warn when yielding a computed value
--FILE--
<?php
function &numbers() {
    yield 2;
}
$generator = numbers();
foreach ($generator as &$value) {
    $value = 10;
    var_dump($generator->current());
}
--EXPECTF--

Notice: Only variable references should be yielded by reference in %s on line 3
int(10)
