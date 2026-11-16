--TEST--
warning_string_write
--FILE--
<?php
$s='abc';$s[0]='XY';echo $s;
--EXPECTF--

Warning: Only the first byte will be assigned to the string offset in %s on line 2
Xbc