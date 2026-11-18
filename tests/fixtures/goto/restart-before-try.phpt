--TEST--
Jumping back to a label before a try runs its finally each time
--FILE--
<?php
$n = 0;
again:
try {
    echo ++$n, "\n";
    if ($n < 2) {
        goto again;
    }
} finally {
    echo "finally\n";
}
--EXPECT--
1
finally
2
finally
