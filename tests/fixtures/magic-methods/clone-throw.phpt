--TEST--
A thrown exception from __clone propagates
--FILE--
<?php
class Copy { function __clone() { throw new Exception("clone failure"); } }
try { clone new Copy; } catch (Exception $e) { echo $e->getMessage(); }
--EXPECT--
clone failure
