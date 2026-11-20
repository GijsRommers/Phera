--TEST--
global $this is a compile error
--FILE--
<?php
class A { function f() { global $this; } }
--EXPECTF--

Fatal error: Cannot use $this as global variable in %s on line %d
