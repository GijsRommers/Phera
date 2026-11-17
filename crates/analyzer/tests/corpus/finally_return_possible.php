<?php
// Rule: control flow / variable.undefined / return; finally_return_possible.
function f($flag) {
    try {
        if ($flag) { return; }
        $x = 2;
    } finally { echo $x; } // error1
}
