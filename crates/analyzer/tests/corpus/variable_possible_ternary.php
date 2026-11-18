<?php
// Rule: variable.undefined; variable possible ternary.
rand() ? ($x = 1) : 0;
echo $x; // error1
