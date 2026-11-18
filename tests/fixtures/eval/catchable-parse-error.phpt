--TEST--
Catchable parse error
--FILE--
<?php
try { eval('if ('); } catch (ParseError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Unclosed '('
