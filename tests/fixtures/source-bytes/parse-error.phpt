--TEST--
A binary comment preserves the location of a later parse error
--FILE--
<?php
// ÿ
echo (1];
--EXPECTF--

Parse error: Unclosed '(' does not match ']' in %s on line 3
