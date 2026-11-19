<?php
// Rule: class.notFound; guard_class_constant_bad.
use Optional\Ext as Component;
class_exists(Component::class);
new Component();
