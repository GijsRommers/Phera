<?php
// Rule: variable.undefined; variable possible or.
rand() || ($x = 1);
echo $x; // error1
