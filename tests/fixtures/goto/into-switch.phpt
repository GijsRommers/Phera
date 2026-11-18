--TEST--
A backward jump into a switch is a compile error
--FILE--
<?php
switch (0) {
default:
    inner:
}
goto inner;
--EXPECTF--
Fatal error: 'goto' into loop or switch statement is disallowed in %s on line 6
