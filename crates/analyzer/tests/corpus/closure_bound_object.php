<?php
// Rule: closure bindings / variable.undefined / arguments.count; closure_bound_object.
class Service { function run() {} }
class Owner {
    function f(Service $service) {
        Closure::bind(function () { $this->run(); }, $service, $service);
    }
}
