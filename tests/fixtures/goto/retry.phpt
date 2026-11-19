--TEST--
A backward goto retries after a caught exception
--FILE--
<?php
function retry(int $attempts, callable $work) {
    start:
    try {
        return $work();
    } catch (RuntimeException $error) {
        if (--$attempts === 0) {
            throw $error;
        }
        goto start;
    }
}
$n = 0;
echo retry(3, function () use (&$n) {
    echo ++$n, "\n";
    if ($n < 3) {
        throw new RuntimeException("retry");
    }
    return "done";
}), "\n";
--EXPECT--
1
2
3
done
