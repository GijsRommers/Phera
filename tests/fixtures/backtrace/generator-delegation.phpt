--TEST--
Suspended delegate callers appear in order
--FILE--
<?php
function inner($value) {
    debug_print_backtrace();
    yield 1;
}
function middle($value) { yield from inner($value); }
function outer($value) { yield from middle($value); }
outer('sample')->rewind();
--EXPECTF--
#0 %s/generator-delegation.php(6): inner('sample')
#1 %s/generator-delegation.php(7): middle('sample')
#2 [internal function]: outer('sample')
#3 %s/generator-delegation.php(8): Generator->rewind()
