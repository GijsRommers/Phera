<?php
// Rule: Constant branch, ternary and loop conditions are diagnosed at their condition.
function checkBranches(bool $flag): void {
    if (true) {
        echo 1;
    } else {
        echo 2;
    }
    if ($flag) {
        echo 3;
    } elseif (false) {
        echo 4;
    }
    echo true ? 5 : 6;
    while (false) {
        echo 7;
    }
    do {
        echo 8;
    } while (false);
    foreach ([] as $unused) {
        echo $unused;
    }
}
