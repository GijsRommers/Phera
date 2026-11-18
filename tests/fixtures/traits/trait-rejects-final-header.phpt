--TEST--
traits: trait rejects final header
--FILE--
<?php
final trait T {}
--EXPECTF--
Parse error: syntax error, unexpected token "trait", expecting "abstract" or "final" or "readonly" or "class" in %s on line 2
