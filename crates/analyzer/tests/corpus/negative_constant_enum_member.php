<?php
// Rule: classConstant.notFound; deliberate single error: constant_enum_member.
enum E { case A; } echo E::UNAVAILABLE;
