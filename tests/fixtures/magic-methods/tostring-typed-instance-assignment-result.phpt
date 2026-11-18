--TEST--
Stringable assignment to a typed instance property returns the converted value
--FILE--
<?php
class Text { function __toString() { echo "cast "; return 'label'; } }
class Holder { public static string $shared = ''; public string $value = ''; }
$o = new Holder; $text = new Text; var_dump($o->value = $text, $o->value, $text instanceof Text);
--EXPECT--
cast string(5) "label"
string(5) "label"
bool(true)
