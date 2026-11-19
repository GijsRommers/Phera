<?php
// Rule: Constant branch, ternary and loop conditions are diagnosed at their condition.
function checkBranches(bool $flag, bool $other, array $items): void {
    if ($flag) {
        echo 1;
    } elseif ($other) {
        echo 2;
    } else {
        echo 3;
    }
    echo $flag ? 4 : 5;
    while ($flag) {
        break;
    }
    do {
        echo 6;
    } while ($other);
    foreach ($items as $item) {
        echo $item;
    }
}
