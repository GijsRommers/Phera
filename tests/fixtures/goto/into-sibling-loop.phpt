--TEST--
A jump between sibling loops is a compile error
--FILE--
<?php
while (false) {
    goto sibling;
}
do {
    sibling:
} while (false);
--EXPECTF--
Fatal error: 'goto' into loop or switch statement is disallowed in %s on line 3
