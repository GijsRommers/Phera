<?php
// Rule: control flow / variable.undefined / return; loop_nested_break.
do {
    while (true) { $x = 1; break 2; }
} while (false);
echo $x;
