<?php
// Rule: closure bindings / variable.undefined / arguments.count; closure_bound_arguments.
class Service { function run($required) {} }
Closure::bind(function () { $this->run(); }, new Service, Service::class); // error0
