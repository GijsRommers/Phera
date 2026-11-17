<?php
// Rule: variable.undefined; variable good early guard.
function f($x) { if (!$x) { return; } echo $x; }
