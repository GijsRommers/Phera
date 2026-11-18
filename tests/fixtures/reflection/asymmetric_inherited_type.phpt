--TEST--
An overridden asymmetric property preserves its inherited type
--FILE--
<?php
class Base { public protected(set) int $value; }
class Record extends Base {
    public protected(set) string $value;
}
--EXPECTF--
Fatal error: Type of Record::$value must be int (as in class Base) in /private%s on line 3
