<?php
// Rule: return.void / return.missing; return good switch.
function f($x): int { switch ($x) { case 1: return 1; default: return 2; } }
