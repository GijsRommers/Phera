--TEST--
Releasing a suspended generator runs nested pending finally blocks
--FILE--
<?php
function hold() {
    try {
        try {
            yield 'held';
            echo "after yield\n";
        } finally {
            echo "inner finally\n";
        }
        echo "after finally\n";
    } finally {
        echo "outer finally\n";
        return 'ignored';
    }
}
$generator = hold();
echo $generator->current(), "\n";
unset($generator);
echo "released\n";
--EXPECT--
held
inner finally
outer finally
released
