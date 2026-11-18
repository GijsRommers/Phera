<?php
// Rule: return.void / return.missing; return good if throw.
function f($x): int { if ($x) { return 1; } else { throw new Exception; } }
