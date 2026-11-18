<?php
// Rule: return.void / return.missing; return bad void null.
function f(): void { return null; } // error1
