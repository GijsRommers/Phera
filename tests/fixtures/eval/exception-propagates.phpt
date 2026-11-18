--TEST--
Exception propagates
--FILE--
<?php
try { eval('throw new RuntimeException("evaluated");'); } catch (RuntimeException $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
evaluated
