--TEST--
Global binding
--FILE--
<?php
$x = 5; function f() { eval('global $x; $x = 9;'); } f(); var_dump($x);
--EXPECT--
int(9)
