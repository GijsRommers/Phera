<?php
// Rule: closure bindings / variable.undefined / arguments.count; closure_bound_null.
class Owner {
    function f() {
        Closure::bind(function () { echo $this; }, null); // error0
    }
}
