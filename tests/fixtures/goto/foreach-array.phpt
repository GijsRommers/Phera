--TEST--
A jump out of a foreach discards the old traversal after array writes
--FILE--
<?php
$a = [1, 2];
foreach ($a as $v) {
    echo $v, "\n";
    $a[] = 3;
    goto done;
}
done:
foreach ($a as $v) {
    echo $v, "\n";
}
--EXPECT--
1
1
2
3
