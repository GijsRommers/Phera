<?php
// Rule: return.void / return.missing; return good finally.
function f(): int { try {} finally { return 1; } }
