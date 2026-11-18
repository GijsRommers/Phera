<?php
// Rule: variable.undefined; variable good reference write.
$a =& $b; $a = 1; echo $b;
