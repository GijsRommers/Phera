<?php
// Rule: class.notFound; guard_class_constant_good.
use Optional\Ext as Component;
if (class_exists(Component::class)) {
    new Component();
}
