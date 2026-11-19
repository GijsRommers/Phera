--TEST--
A generator cannot declare never
--FILE--
<?php
function numbers(): never {
    yield 1;
}
--EXPECTF--

Fatal error: Generator return type must be a supertype of Generator, never given in %s on line 2
