<?php
// Rule: return.void / return.missing; return bad nullable missing.
function f(): ?int {} // error1
