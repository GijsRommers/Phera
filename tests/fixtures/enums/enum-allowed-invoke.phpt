--TEST--
enums: enum allowed invoke
--FILE--
<?php
enum E{case A; function __invoke(){return $this->name;}} echo E::A->__invoke();
--EXPECT--
A
