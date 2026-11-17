<?php
// Rule: control flow / variable.undefined / return; return_void_throw.
function f(): void { return throw new Exception; } // error1
