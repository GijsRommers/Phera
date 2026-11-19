--TEST--
A reserved token cannot name a goto target
--FILE--
<?php
goto die;
--EXPECTF--
Parse error: syntax error, unexpected token "exit", expecting identifier in %s on line 2
