<?php
// Rule: class.notFound; deliberate single error: class_extends_alias.
use Unknown\Base as ParentClass; class C extends ParentClass {}
