<?php
// Rule: variable.undefined; variable possible and.
rand() && ($x = 1);
echo $x; // error1
