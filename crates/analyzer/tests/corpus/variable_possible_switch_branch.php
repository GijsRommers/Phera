<?php
// Rule: variable.undefined; variable possible switch branch.
switch (rand()) { case 1: break; default: $x = 1; }
echo $x; // error1
