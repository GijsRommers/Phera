--TEST--
A control-flow keyword before a colon retains its grammar role
--FILE--
<?php
if:
--EXPECTF--
Parse error: syntax error, unexpected token ":", expecting "(" in %s on line 2
