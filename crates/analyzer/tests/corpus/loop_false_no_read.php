<?php
// Rule: control flow / variable.undefined / return; loop_false_no_read.
while (false) { echo $missing; }
for (; false;) { echo $missing; }
