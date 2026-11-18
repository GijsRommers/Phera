--TEST--
enums: instance method
--FILE--
<?php
enum E{case A;function label(){return $this->name;}} echo E::A->label();
--EXPECT--
A
