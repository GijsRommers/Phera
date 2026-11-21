--TEST--
Visible backed properties preserve the inherited type
--FILE--
<?php
class ParentType {
    protected int $value;
}
class ChildType extends ParentType {
    public string $value;
}
--EXPECTF--
Fatal error: Type of ChildType::$value must be int (as in class ParentType) in %s on line 5
