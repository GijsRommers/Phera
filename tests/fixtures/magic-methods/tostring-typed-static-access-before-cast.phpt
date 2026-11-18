--TEST--
A forbidden static write fails before running __toString
--FILE--
<?php
class Text { function __toString() { echo "cast "; return 'label'; } }
class Holder { public static string $shared = ''; public string $value = ''; }
class Hidden { private static string $label = ''; } try { Hidden::$label = new Text; } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Cannot access private property Hidden::$label
