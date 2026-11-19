--TEST--
A void setter cannot yield
--FILE--
<?php
class Source {
    public $value {
        set => yield $value;
    }
}
--EXPECTF--

Fatal error: Generator return type must be a supertype of Generator, void given in %s on line 4
