<?php
// Rule: variable.undefined; variable good finally write.
try {} finally { $x = 1; } echo $x;
