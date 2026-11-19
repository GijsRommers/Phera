<?php
// Rule: variable.undefined; coalesce_scope_bad.
function read(bool $flag) {
    if ($flag) { $val = strlen('value'); }
    return ($val ?? 0) + $val;
}
