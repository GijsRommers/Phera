<?php
// Rule: control flow / variable.undefined / return; finally_return_scopes.
function f($flag) {
    try {
        if ($flag) { $x = 1; return; }
        else { $x = 2; return; }
    } finally { echo $x; }
}
