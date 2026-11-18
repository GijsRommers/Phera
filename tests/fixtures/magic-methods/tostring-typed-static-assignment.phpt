--TEST--
Weak assignment casts a static property value and returns the stored string
--FILE--
<?php
class Text { function __toString() { echo "cast "; return 'label'; } }
class Holder { public static string $shared = ''; public string $value = ''; }
$text = new Text; var_dump(Holder::$shared = $text, Holder::$shared);
--EXPECT--
cast string(5) "label"
string(5) "label"
