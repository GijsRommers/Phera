--TEST--
Label lookup is case sensitive
--FILE--
<?php
a:
goto A;
--EXPECTF--
Fatal error: 'goto' to undefined label 'A' in %s on line 3
