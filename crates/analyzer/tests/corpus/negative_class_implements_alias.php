<?php
// Rule: class.notFound; deliberate single error: class_implements_alias.
use Unknown\Contract as I; class C implements I {}
