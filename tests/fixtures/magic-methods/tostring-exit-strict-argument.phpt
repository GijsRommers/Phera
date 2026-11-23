--TEST--
Strict exit rejects a stringable status before invoking the conversion
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { echo 'cast'; return 'status'; } } try { exit(new Text); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
exit(): Argument #1 ($status) must be of type string|int, Text given
