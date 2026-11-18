<?php
// Rule: names / global scope / callable; globals_called_write.
function f() { $GLOBALS["created"] = 1; } f(); echo $created;
