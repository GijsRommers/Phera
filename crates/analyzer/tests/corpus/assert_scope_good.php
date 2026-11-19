<?php
// Rule: variable.undefined; assert_scope_good.
function render(bool $flag) {
    if ($flag) { $value = strlen('yes'); }
    assert(isset($value));
    echo $value;
}
