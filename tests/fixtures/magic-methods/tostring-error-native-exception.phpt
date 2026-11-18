--TEST--
tostring error native exception
--FILE--
<?php
class Text { function __toString() { throw new Exception("render"); } }
try { strlen(new Text); } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
render
