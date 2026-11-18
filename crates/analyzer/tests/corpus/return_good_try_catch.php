<?php
// Rule: return.void / return.missing; return good try catch.
function f(): int { try { return 1; } catch (Exception $e) { return 2; } }
