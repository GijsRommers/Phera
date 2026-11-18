<?php
// Rule: variable.undefined; variable possible switch.
switch (rand()) { case 1: $x = 1; break; }
echo $x; // error1
