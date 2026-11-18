<?php
// Rule: control flow / variable.undefined / return; dynamic_constant_name_read.
class C {}
echo C::{$missing}; // error0
