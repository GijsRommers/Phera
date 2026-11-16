--TEST--
Application class sweep: namespaced-alias
--FILE--
<?php
namespace Domain {
class User {
function __construct(
public int $id)
{
}
static function create(
int $id)
: self {
return new self(
$id);
}
}
}
 namespace App {
use Domain\User as Member;
class Controller {
function load(): Member {
return Member::create(12);
}
}
$controller=new Controller;
$member=$controller->load();
echo get_class($member),
'=',
$member->id,
"\n";
echo Member::class,
"\n";
}

--EXPECT--
Domain\User=12
Domain\User
