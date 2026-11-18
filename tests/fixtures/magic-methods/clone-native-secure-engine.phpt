--TEST--
Random Engine Secure refuses clone
--FILE--
<?php
try { clone new Random\Engine\Secure; }
catch (Error $e) { echo $e->getMessage(); }
--EXPECT--
Trying to clone an uncloneable object of class Random\Engine\Secure
