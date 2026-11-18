<?php
// Rule: variable.undefined; variable good if terminates.
if (rand()) { $x = 1; } else { exit; } echo $x;
