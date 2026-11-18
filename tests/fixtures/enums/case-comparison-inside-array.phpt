--TEST--
enums: case comparison inside array
--FILE--
<?php
enum E {case A;case B;} var_dump([E::A] == [E::A], [E::A] === [E::A], [E::A] == [E::B], [E::A] < [E::B], [E::B] < [E::A]);
--EXPECT--
bool(true)
bool(true)
bool(false)
bool(false)
bool(false)
