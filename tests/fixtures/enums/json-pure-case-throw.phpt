--TEST--
enums: json pure case throw
--FILE--
<?php
enum E{case A;} try{json_encode(E::A,JSON_THROW_ON_ERROR);}catch(JsonException $e){echo $e->getCode()," ",$e->getMessage();}
--EXPECT--
11 Non-backed enums have no default serialization
