--TEST--
A generator cannot declare void
--FILE--
<?php
function numbers(): void {
    return 2;
    yield 1;
}
--EXPECTF--

Fatal error: Generator return type must be a supertype of Generator, void given in %s on line 2
