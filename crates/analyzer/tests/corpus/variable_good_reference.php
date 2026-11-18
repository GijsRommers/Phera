<?php
// Rule: variable.undefined; variable good reference.
$a =& $b; echo $a; echo $b;
