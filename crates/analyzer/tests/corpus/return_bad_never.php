<?php
// Rule: return.void / return.missing; return bad never.
function f(): never {} // error1
