<?php
// Rule: class.noParent; a class without a base cannot make a parent call.
class Root { function f() { parent::run(); } }
