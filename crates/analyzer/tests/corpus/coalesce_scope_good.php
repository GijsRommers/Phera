<?php
// Rule: variable.undefined; coalesce_scope_good.
function read(bool $flag) {
    if ($flag) { $val = strlen('value'); }
    return ($val ?? throw new RuntimeException()) ? $val : 0;
}
