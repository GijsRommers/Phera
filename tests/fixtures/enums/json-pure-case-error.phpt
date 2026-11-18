--TEST--
enums: json pure case error
--FILE--
<?php
enum E{case A;} var_dump(json_encode(E::A),json_last_error(),json_last_error_msg());
--EXPECT--
bool(false)
int(11)
string(46) "Non-backed enums have no default serialization"
