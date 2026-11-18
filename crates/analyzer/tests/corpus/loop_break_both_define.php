<?php
// Rule: control flow / variable.undefined / return; loop_break_both_define.
do {
    if (rand(0, 1)) { $x = 1; break; }
    $x = 2;
} while (false);
echo $x;
