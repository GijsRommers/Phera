<?php
// Rule: staticProperty.notFound; deliberate single error: property_class_static.
class C {} echo C::$unavailable;
