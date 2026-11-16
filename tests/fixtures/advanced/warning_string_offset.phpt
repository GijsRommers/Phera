--TEST--
warning_string_offset
--FILE--
<?php
$s='abc';echo $s[9];
--EXPECTF--

Warning: Uninitialized string offset 9 in %s on line 2
