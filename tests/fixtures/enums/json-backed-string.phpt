--TEST--
enums: json backed string
--FILE--
<?php
enum E: string{case A="one";} echo json_encode(E::A);
--EXPECT--
"one"
