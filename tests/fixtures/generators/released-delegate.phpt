--TEST--
A generator releases a delegated temporary before its own finally
--FILE--
<?php
function inner() {
    try { yield 'held'; }
    finally { echo "inner finally\n"; }
}
function outer() {
    try { yield from inner(); }
    finally { echo "outer finally\n"; }
}
$generator = outer();
echo $generator->current(), "\n";
unset($generator);
echo "released\n";
--EXPECT--
held
inner finally
outer finally
released
