--TEST--
A forward jump into a loop is a compile error
--FILE--
<?php
goto inner;
while (false) {
    inner:
}
--EXPECTF--
Fatal error: 'goto' into loop or switch statement is disallowed in %s on line 2
