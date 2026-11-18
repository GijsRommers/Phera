--TEST--
tostring strict parameter
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { return "hello"; } }
function accept(string $v) {} try { accept(new Text); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECTF--
accept(): Argument #1 ($v) must be of type string, Text given, called in %s on line 3
