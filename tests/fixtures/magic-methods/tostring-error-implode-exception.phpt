--TEST--
tostring error implode exception
--FILE--
<?php
class Text { function __toString() { throw new Exception("render"); } }
try { implode([new Text]); } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
render
