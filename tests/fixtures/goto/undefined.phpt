--TEST--
An undefined label reports the line of the target identifier
--FILE--
<?php
goto
    missing
    ;
--EXPECTF--
Fatal error: 'goto' to undefined label 'missing' in %s on line 3
