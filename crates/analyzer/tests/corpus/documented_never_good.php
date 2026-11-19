<?php
// Rule: return.missing; documented_never_good.
/** @return never */
function stopNow() { throw new RuntimeException(); }
function readValue(): int {
    stopNow();
}
