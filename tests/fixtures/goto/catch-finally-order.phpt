--TEST--
A goto from a catch runs each finally it leaves
--FILE--
<?php
try {
    try {
        throw new RuntimeException("failed");
    } catch (RuntimeException $e) {
        goto done;
    } finally {
        echo "inner\n";
    }
} finally {
    echo "outer\n";
}
done:
echo "done\n";
--EXPECT--
inner
outer
done
