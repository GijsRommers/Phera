--TEST--
enums: a deprecated case is reported on each read
--FILE--
<?php
enum Dep { #[Deprecated("use Fresh")] case Old; case Fresh; }
set_error_handler(function (int $level, string $message): bool { echo "  ", $message, "\n"; return true; });
function reads(): int { $n = 0; for ($i = 0; $i < 3; $i++) { $n += Dep::Old === Dep::Old ? 1 : 0; $n += Dep::Fresh === Dep::Fresh ? 1 : 0; } return $n; }
var_dump(reads());
var_dump(count(Dep::cases()), Dep::Old->name);
--EXPECT--
  Enum case Dep::Old is deprecated, use Fresh
  Enum case Dep::Old is deprecated, use Fresh
  Enum case Dep::Old is deprecated, use Fresh
  Enum case Dep::Old is deprecated, use Fresh
  Enum case Dep::Old is deprecated, use Fresh
  Enum case Dep::Old is deprecated, use Fresh
int(6)
  Enum case Dep::Old is deprecated, use Fresh
int(2)
string(3) "Old"
