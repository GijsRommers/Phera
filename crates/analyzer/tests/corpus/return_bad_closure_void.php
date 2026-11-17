<?php
// Rule: return.void / return.missing; return bad closure void.
$f = function (): void { return 1; }; // error1
