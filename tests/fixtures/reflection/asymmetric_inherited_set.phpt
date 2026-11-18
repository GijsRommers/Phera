--TEST--
An inherited public setter cannot be restricted
--FILE--
<?php
class Base { public int $value; }
class Record extends Base {
    public protected(set) int $value;
}
--EXPECTF--
Fatal error: Set access level of Record::$value must be omitted (as in class Base) in %s on line 3
