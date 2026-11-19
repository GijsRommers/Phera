--TEST--
A jump into a finally block is a compile error
--FILE--
<?php
goto cleanup;
try {
    echo "try\n";
} finally {
    cleanup:
}
--EXPECTF--
Fatal error: jump into a finally block is disallowed in %s on line 2
