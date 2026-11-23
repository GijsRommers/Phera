--TEST--
A concrete constructor preserves an inherited abstract signature
--FILE--
<?php
class Accepted {}
abstract class ParentType {
    abstract public function __construct(Accepted $value);
}
class MiddleType extends ParentType {
    public function __construct(object $value) {}
}
class ChildType extends MiddleType {
    public function __construct(Unavailable $value) {}
}
--EXPECTF--
Fatal error: Could not check compatibility between ChildType::__construct(Unavailable $value) and ParentType::__construct(Accepted $value), because class Unavailable is not available in %s on line 10
