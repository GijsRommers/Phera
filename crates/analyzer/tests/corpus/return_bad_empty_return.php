<?php
// Rule: return.void / return.missing; return bad empty return.
function f(): int { return; } // error1
