<?php
// Rule: arguments.count / argument.unknown; arguments bad constructor.
class C { function __construct($a) {} } new C; // error0
