<?php
// Rule: control flow / variable.undefined / return; loop_break_maybe_define.
do {
    if (rand(0, 1)) { break; }
    $x = 2;
} while (false);
echo $x; // error1
