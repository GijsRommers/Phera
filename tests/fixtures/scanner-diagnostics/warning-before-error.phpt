--TEST--
A scanner warning is printed before a later parse error in the file
--FILE--
<?php
echo "\400";
echo (1];
--EXPECTF--

Warning: Octal escape sequence overflow \400 is greater than \377 in %s on line 2

Parse error: Unclosed '(' does not match ']' in %s on line 3
