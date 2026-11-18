--TEST--
enums: json backed int
--FILE--
<?php
enum E: int{case A=8;} echo json_encode(E::A);
--EXPECT--
8
