<?php
// Rule: control flow / variable.undefined / return; finally_override_return.
function f(): int {
    try { return 1; } finally { throw new Exception; }
}
