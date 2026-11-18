--TEST--
A property cannot declare more than one set visibility
--FILE--
<?php
class Record {
    public protected(set) private(set) int $value;
}
--EXPECTF--
Fatal error: Multiple access type modifiers are not allowed in /private%s on line 3
