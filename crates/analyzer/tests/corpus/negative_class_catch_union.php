<?php
// Rule: class.notFound; deliberate single error: class_catch_union.
try { strlen("x"); } catch (Exception|Absent $error) {}
