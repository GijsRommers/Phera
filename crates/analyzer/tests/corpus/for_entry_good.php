<?php
// Rule: variable.undefined; for_entry_good.
function render() {
    for ($i = 0; $i < 1; ++$i) { $value = strlen('yes'); }
    echo $value;
}
