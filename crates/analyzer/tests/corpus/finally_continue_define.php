<?php
// Rule: control flow / variable.undefined / return; finally_continue_define.
do {
    try { continue; } finally { $x = 1; }
} while (false);
echo $x;
