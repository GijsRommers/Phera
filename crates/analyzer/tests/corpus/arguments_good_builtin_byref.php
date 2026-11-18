<?php
// Rule: arguments.count / argument.unknown; arguments good builtin byref.
preg_match("/a/", "a", $matches); echo $matches;
