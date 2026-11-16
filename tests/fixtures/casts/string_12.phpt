--TEST--
string_12
--FILE--
<?php
echo (string)[1], "\n";
--EXPECTF--

Warning: Array to string conversion in %s on line 2
Array
