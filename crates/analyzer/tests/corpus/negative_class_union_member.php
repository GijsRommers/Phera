<?php
// Rule: class.notFound; deliberate single error: class_union_member.
function f(DateTime|Absent $value) {}
