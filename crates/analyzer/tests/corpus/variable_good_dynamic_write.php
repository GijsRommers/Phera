<?php
// Rule: variable.undefined; variable good dynamic write.
$name = "x"; $$name = 1; echo $x;
