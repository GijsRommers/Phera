--TEST--
An inherited protected setter cannot be made private
--FILE--
<?php
class Base { public protected(set) int $value; }
class Record extends Base {
    public private(set) int $value;
}
--EXPECTF--
Fatal error: Set access level of Record::$value must be protected(set) (as in class Base) or weaker in %s on line 3
