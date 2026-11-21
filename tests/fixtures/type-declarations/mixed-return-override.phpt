--TEST--
A method returning void cannot override a mixed return contract
--FILE--
<?php
class ParentType {
    public function value(): mixed { return null; }
}
class ChildType extends ParentType {
    public function value(): void {}
}
--EXPECTF--
Fatal error: Declaration of ChildType::value(): void must be compatible with ParentType::value(): mixed in %s on line 6
