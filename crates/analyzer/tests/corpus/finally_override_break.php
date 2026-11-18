<?php
// Rule: control flow / variable.undefined / return; finally_override_break.
function f(): int {
    while (true) {
        try { break; } finally { return 1; }
    }
}
