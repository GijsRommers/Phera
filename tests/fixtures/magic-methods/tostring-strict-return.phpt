--TEST--
tostring strict return
--FILE--
<?php
declare(strict_types=1); class Text { function __toString() { return "hello"; } }
function result(): string { return new Text; } try { result(); } catch (TypeError $e) { echo $e->getMessage(); }
--EXPECT--
result(): Return value must be of type string, Text returned
