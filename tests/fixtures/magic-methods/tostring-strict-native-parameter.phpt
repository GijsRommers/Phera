--TEST--
tostring strict native parameter
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { return "hello"; } }
try { strlen(new Text); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
strlen(): Argument #1 ($string) must be of type string, Text given
