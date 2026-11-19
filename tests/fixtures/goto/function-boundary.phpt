--TEST--
A function cannot jump to a file-scope label
--FILE--
<?php
target:
function work() {
    goto target;
}
--EXPECTF--
Fatal error: 'goto' to undefined label 'target' in %s on line 4
