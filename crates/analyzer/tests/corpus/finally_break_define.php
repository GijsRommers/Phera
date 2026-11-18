<?php
// Rule: control flow / variable.undefined / return; finally_break_define.
while (true) {
    try { break; } finally { $x = 1; }
}
echo $x;
