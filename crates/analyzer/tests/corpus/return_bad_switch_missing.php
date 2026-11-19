<?php
// Rule: return.void / return.missing; return bad switch missing.
function f($x): int { switch ($x) { case 1: return 1; } } // error0
