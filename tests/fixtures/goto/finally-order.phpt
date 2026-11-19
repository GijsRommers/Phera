--TEST--
A goto leaves finally blocks from innermost to outermost
--FILE--
<?php
try {
    try {
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
