<?php
// Rule: arguments.count / argument.unknown; arguments good byref.
function f(&$a) {} f($new); echo $new;
