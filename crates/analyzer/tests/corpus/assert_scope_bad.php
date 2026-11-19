<?php
// Rule: variable.undefined; checking an unrelated expression leaves $value uncertain.
function render(bool $flag, $guard) {
    if ($flag) { $value = strlen('yes'); }
    assert($guard);
    echo $value;
}
