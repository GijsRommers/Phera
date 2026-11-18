--TEST--
enums: match on case
--FILE--
<?php
enum E{case A;case B;} echo match(E::B){E::A=>"a",E::B=>"b"};
--EXPECT--
b
