<?php
// Rule: variable.undefined; variable good extract unknown.
function f($input) { extract($input); echo $x; }
