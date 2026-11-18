--TEST--
Non-invokable objects cannot be called
--FILE--
<?php
$o = new stdClass;
try { $o(); } catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Object of type stdClass is not callable
