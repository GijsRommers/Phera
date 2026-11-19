--TEST--
Generator exception traces reflect foreach, methods and delegation
--FILE--
<?php
function inner() { throw new Exception; yield; }
function outer() { yield from inner(); }
try {
    foreach (inner() as $value) {}
} catch (Exception $error) {
    echo $error->getTraceAsString(), "\n";
}
try {
    inner()->current();
} catch (Exception $error) {
    echo $error->getTraceAsString(), "\n";
}
try {
    outer()->current();
} catch (Exception $error) {
    echo $error->getTraceAsString(), "\n";
}
--EXPECTF--
#0 %s(5): inner()
#1 {main}
#0 [internal function]: inner()
#1 %s(10): Generator->current()
#2 {main}
#0 %s(3): inner()
#1 [internal function]: outer()
#2 %s(15): Generator->current()
#3 {main}
