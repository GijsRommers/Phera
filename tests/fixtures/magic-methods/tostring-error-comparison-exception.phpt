--TEST--
tostring error comparison exception
--FILE--
<?php
class Text { function __toString() { throw new Exception("render"); } }
try { var_dump(new Text == "x"); } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
render
