<?php
// Rule: return.void / return.missing; return bad branch missing.
function f($x): int { if ($x) { return 1; } } // error1
