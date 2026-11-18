<?php
// Rule: return.void / return.missing; return bad void value.
function f(): void { return 1; } // error1
