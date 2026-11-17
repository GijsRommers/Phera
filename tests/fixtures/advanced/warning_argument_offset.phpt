--TEST--
warning_argument_offset
--FILE--
<?php
$s='a';var_dump($s[3]);
--EXPECTF--

Warning: Uninitialized string offset 3 in %s on line 2
string(0) ""
