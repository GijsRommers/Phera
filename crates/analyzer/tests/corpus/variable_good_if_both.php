<?php
// Rule: variable.undefined; variable good if both.
if (rand()) { $x = 1; } else { $x = 2; } echo $x;
