--TEST--
Include hoists class
--FILE--
<?php
require __DIR__ . "/early-included.inc";
--EXPECT--
bool(true)
