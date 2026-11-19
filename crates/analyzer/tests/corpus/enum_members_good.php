<?php
// Rule: arguments.count; enum_members_good.
enum Colour { case Red; function shade($percent) {} }
Colour::Red->shade(20);
