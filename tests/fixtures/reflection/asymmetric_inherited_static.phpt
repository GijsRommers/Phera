--TEST--
A final asymmetric property cannot be redeclared as static
--FILE--
<?php
class Base { public private(set) int $value; }
class Record extends Base {
    public static int $value;
}
--EXPECTF--
Fatal error: Cannot override final property Base::$value in /private%s on line 3
