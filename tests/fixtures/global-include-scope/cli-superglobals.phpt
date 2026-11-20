--TEST--
CLI request superglobals exist in the global table
--FILE--
<?php
var_dump($GLOBALS["_GET"], $GLOBALS["_POST"], $GLOBALS["_COOKIE"]);
--EXPECT--
array(0) {
}
array(0) {
}
array(0) {
}
