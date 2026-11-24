<?php
const TRACE_LABEL = "debugger";
class Packet {
    protected string $label = 'packet';
    private ?int $missing;
    public static int $instances = 1;
    public function __construct(public readonly array $body) {}
    public function size(): int {
        $count = count($this->body);
        return $count;
    }
}
function inspect(Packet $packet): int {
    $data = ['nested' => ['one' => [10, 20, 30]], 'bin' => "a\0b\xff"];
    $alias =& $data;
    $closure = static fn ($value) => $value + 1;
    $count = $packet->size();
    $result = $closure($count);
    return $result;
}
$packet = new Packet(['element' => 1]);
$count = inspect($packet);
echo $count, "\n";
