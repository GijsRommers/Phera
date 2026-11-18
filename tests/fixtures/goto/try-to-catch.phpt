--TEST--
A jump between try and catch runs finally only once
--FILE--
<?php
try {
    goto caught;
} catch (RuntimeException $e) {
    caught: echo "caught\n";
} finally {
    echo "finally\n";
}
--EXPECT--
caught
finally
