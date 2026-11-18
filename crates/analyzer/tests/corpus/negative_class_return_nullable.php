<?php
// Rule: class.notFound; deliberate single error: class_return_nullable.
function f(): ?Absent { return null; }
