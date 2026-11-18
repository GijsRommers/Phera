--TEST--
An asymmetrically visible promoted readonly property requires a type
--FILE--
<?php
class Record {
    public function __construct(public private(set) readonly $value) {}
}
--EXPECTF--
Fatal error: Readonly property Record::$value must have type in /private%s on line 3
