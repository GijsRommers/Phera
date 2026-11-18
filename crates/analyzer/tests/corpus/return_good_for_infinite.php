<?php
// Rule: return.void / return.missing; return good for infinite.
function f(): int { for (;;) {} }
