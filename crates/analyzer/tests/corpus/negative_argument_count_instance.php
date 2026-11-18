<?php
// Rule: arguments.count; deliberate single error: argument_count_instance.
class C { function target($first) {} function f() { $this->target(); } }
