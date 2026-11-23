<?php
// Rule: return.missing; documented_never_bad.
function stopNow(): void {}
function readValue(): int {
    stopNow();
}
