--TEST--
Mismatched delimiter
--FILE--
<?php
try { eval('[1, 2)];'); } catch(ParseError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Unclosed '[' does not match ')'
