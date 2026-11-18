--TEST--
A promoted parameter cannot declare more than one set visibility
--FILE--
<?php
class Record {
    public function __construct(public protected(set) private(set) int $value) {}
}
--EXPECTF--
Fatal error: Multiple access type modifiers are not allowed in %s on line 3
