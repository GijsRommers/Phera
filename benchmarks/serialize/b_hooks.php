<?php
class CacheEntry
{
    public function __construct(
        private int $id,
        protected string $tag,
        public float $weight,
    ) {}

    public function __serialize(): array
    {
        return ['id' => $this->id, 'tag' => $this->tag, 'weight' => $this->weight];
    }

    public function __unserialize(array $data): void
    {
        $this->id = $data['id'];
        $this->tag = $data['tag'];
        $this->weight = $data['weight'];
    }
}

$list = [];
for ($i = 0; $i < 10000; $i++) {
    $list[] = new CacheEntry($i, 'cache', 3.5);
}
$n = 0;
for ($r = 0; $r < 30; $r++) {
    $s = serialize($list);
    $n += strlen($s);
    $u = unserialize($s);
    $n += count($u);
}
echo $n, "\n";
