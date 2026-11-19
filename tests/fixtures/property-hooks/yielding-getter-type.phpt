--TEST--
A yielding getter requires a generator-compatible property type
--FILE--
<?php
class Source {
    public int $value {
        get { yield 1; }
    }
}
--EXPECTF--

Fatal error: Generator return type must be a supertype of Generator, int given in %s on line 4
