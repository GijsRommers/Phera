<?php
// Rule: names / global scope / callable; constant_firstclass_callable.
const C = strlen(...); (C)("value");
