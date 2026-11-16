--TEST--
Application class sweep: class spanning included files
--FILE--
<?php
require __DIR__.'/m3c-included-base.inc';
require __DIR__.'/m3c-included-child.inc';
$first = IncludedChild::make(4);
$second = IncludedChild::make(8);
$records = [
    $first,
    $second,
];
foreach ($records as $record)
{
    echo $record->label();
    echo "\n";
    echo get_class($record);
    echo "\n";
    echo get_parent_class($record);
    echo "\n";
}
function includedIdentity(
    IncludedBase $record
): IncludedBase
{
    return $record;
}
$selected = includedIdentity($second);
echo $selected->value;
echo "\n";
echo $selected === $second ? 'same' : 'different';
echo "\n";
echo IncludedChild::class;
echo "\n";
--EXPECT--
base:4/child
IncludedChild
IncludedBase
base:8/child
IncludedChild
IncludedBase
8
same
IncludedChild
