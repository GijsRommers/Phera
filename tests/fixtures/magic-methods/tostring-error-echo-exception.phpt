--TEST--
tostring error echo exception
--FILE--
<?php
class Text { function __toString() { throw new Exception("render"); } }
try { echo new Text; } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
render
