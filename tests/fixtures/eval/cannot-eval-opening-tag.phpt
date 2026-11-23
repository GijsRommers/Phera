--TEST--
Cannot eval opening tag
--FILE--
<?php
try { eval('<?php echo 1;'); } catch (ParseError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
syntax error, unexpected token "<", expecting end of file
