--TEST--
A weak property write coerces a Stringable object to its string type
--FILE--
<?php
class Text { function __toString() { return "hello"; } }
class Target { public string $text; }
$t = new Target; $t->text = new Text; var_dump($t->text);
--EXPECT--
string(5) "hello"
