--TEST--
enums: json pure case partial
--FILE--
<?php
enum E{case A;} echo json_encode([E::A],JSON_PARTIAL_OUTPUT_ON_ERROR);
--EXPECT--
[0]
