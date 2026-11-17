--TEST--
callable syntax only
--FILE--
<?php
var_dump(is_callable("missing",true,$name),$name,is_callable(["Missing","f"],true,$name),$name);
--EXPECT--
bool(true)
string(7) "missing"
bool(true)
string(10) "Missing::f"
