--TEST--
string_11
--FILE--
<?php
echo (string)[], "\n";
--EXPECTF--

Warning: Array to string conversion in %s on line 2
Array
