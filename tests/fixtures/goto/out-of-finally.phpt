--TEST--
A jump out of a finally block is a compile error
--FILE--
<?php
try {
    echo "try\n";
} finally {
    goto done;
}
done:
--EXPECTF--
Fatal error: jump out of a finally block is disallowed in %s on line 5
