<?php
// Rule: variable.undefined; for_entry_bad.
function render(int $count) {
    for ($i = 0; $i < $count; ++$i) { $value = strlen('yes'); }
    echo $value;
}
