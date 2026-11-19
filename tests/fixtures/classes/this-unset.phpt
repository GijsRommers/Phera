--TEST--
unset($this) is a compile error
--FILE--
<?php
class A { function f() { unset($this); } }
--EXPECTF--

Fatal error: Cannot unset $this in %s on line %d
