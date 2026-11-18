<?php
// Rule: control flow / variable.undefined / return; loop_nested_continue.
do {
    while (true) {
        if (rand(0, 1)) { $x = 1; continue 2; }
        break;
    }
    $x = 2;
} while (false);
echo $x;
