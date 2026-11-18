--TEST--
A goto target cannot be a qualified name
--FILE--
<?php
goto A\B;
--EXPECTF--
Parse error: syntax error, unexpected namespaced name "A\B", expecting identifier in %s on line 2
