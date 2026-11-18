<?php
// Rule: variable.undefined; variable good nested arrow.
$x = 1; $f = fn() => fn() => $x;
