--TEST--
A jump into a try retains its exception handler
--FILE--
<?php
goto inside;
try {
    echo "skipped\n";
    inside: throw new RuntimeException("caught");
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
--EXPECT--
caught
