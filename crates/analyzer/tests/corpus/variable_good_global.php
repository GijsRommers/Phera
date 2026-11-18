<?php
// Rule: variable.undefined; variable good global.
function f() { global $x; echo $x; }
