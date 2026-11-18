--TEST--
enums: format export in array
--FILE--
<?php
enum E{case A;} echo json_encode(var_export(["x"=>E::A], true));
--EXPECT--
"array (\n  'x' => \n  \\E::A,\n)"
