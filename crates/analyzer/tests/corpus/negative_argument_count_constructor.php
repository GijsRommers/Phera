<?php
// Rule: arguments.count; deliberate single error: argument_count_constructor.
class C { function __construct($first) {} } new C;
