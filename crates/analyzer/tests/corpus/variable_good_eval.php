<?php
// Rule: variable.undefined; variable good eval.
eval("$" . "x = 1;"); echo $x;
