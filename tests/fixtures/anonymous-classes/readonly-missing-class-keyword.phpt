--TEST--
An anonymous readonly modifier requires the class keyword
--FILE--
<?php
new readonly
stdClass;
--EXPECTF--
Parse error: syntax error, unexpected identifier "stdClass", expecting "class" in %s on line 3
