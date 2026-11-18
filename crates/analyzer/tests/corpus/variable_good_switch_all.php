<?php
// Rule: variable.undefined; variable good switch all.
switch (rand()) { case 1: $x = 1; break; default: $x = 2; } echo $x;
