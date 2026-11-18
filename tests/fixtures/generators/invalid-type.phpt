--TEST--
Generator return types must accept the Generator object
--FILE--
<?php
function numbers(): int {
    yield 1;
}
--EXPECTF--

Fatal error: Generator return type must be a supertype of Generator, int given in %s on line 2
