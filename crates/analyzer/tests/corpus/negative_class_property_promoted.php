<?php
// Rule: class.notFound; deliberate single error: class_property_promoted.
class C { function __construct(public Absent $value) {} }
