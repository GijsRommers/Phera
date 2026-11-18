--TEST--
A typed property alias casts stringable values before storing and returning them
--FILE--
<?php
class Text { function __toString() { echo "cast "; return 'label'; } }
class Holder { public static string $shared = ''; public string $value = ''; }
$o = new Holder; $alias =& $o->value; $text = new Text; var_dump($alias = $text, $o->value, $text instanceof Text);
--EXPECT--
cast string(5) "label"
string(5) "label"
bool(true)
