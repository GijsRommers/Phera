<?php
// Rule: variable.undefined; variable bad reference read.
$a = 1; $b =& $a; echo $missing; // error0
