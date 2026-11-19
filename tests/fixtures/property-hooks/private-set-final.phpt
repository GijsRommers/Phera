--TEST--
Private set visibility makes a hooked property final
--FILE--
<?php
class ParentBox {
    public private(set) int $value { set => $value; }
}
class ChildBox extends ParentBox {
    public int $value;
}
--EXPECTF--
Fatal error: Cannot override final property ParentBox::$value in %s on line 5
