--TEST--
Parse error source
--FILE--
<?php
try { eval("\nif ("); } catch (ParseError $e) { echo basename($e->getFile()), ":", $e->getLine(), "\n"; }
--EXPECT--
parse-error-source.php(2) : eval()'d code:2
