<?php
// Rule: classConstant.notFound; enum_members_bad.
enum Colour { case Red; function shade($percent) {} }
Colour::Blue;
