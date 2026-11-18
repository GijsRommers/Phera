--TEST--
Anonymous class declarations reject a repeated readonly modifier
--FILE--
<?php
new readonly
readonly class {};
--EXPECTF--
Fatal error: Multiple readonly modifiers are not allowed in %s on line 3
