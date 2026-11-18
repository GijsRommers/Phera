<?php
// Rule: return.void / return.missing; return good if both.
function f($x): int { if ($x) { return 1; } else { return 2; } }
