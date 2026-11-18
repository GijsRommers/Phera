--TEST--
enums: enum trait method
--FILE--
<?php
trait T{function label(){return $this->name;}} enum E{use T;case A;} echo E::A->label();
--EXPECT--
A
