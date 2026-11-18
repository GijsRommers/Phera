--TEST--
Parse error hierarchy
--FILE--
<?php
try { eval('if ('); } catch (CompileError $e) { echo get_class($e), "\n"; }
--EXPECT--
ParseError
