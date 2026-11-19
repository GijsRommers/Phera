<?php
// Exercise the same reflection calls a container and mapper make.
#[Attribute(Attribute::TARGET_CLASS)]
final class ReflectionBenchmarkMapping {
    public function __construct(public string $table) {}
}

#[ReflectionBenchmarkMapping('records')]
final class ReflectionBenchmarkRecord {
    private int $result;

    public function __construct(public int $input) {}
}

function reflectionBenchmark(int $iterations): int {
    $checksum = 0;
    for ($i = 0; $i < $iterations; ++$i) {
        $class = new ReflectionClass(ReflectionBenchmarkRecord::class);
        $properties = $class->getProperties();
        $mapping = $class->getAttributes(ReflectionBenchmarkMapping::class)[0]->newInstance();
        $object = $class->newInstanceArgs(['input' => $i]);
        $properties[0]->setValue($object, $i + 1);
        $checksum += $object->input + $properties[0]->getValue($object) + strlen($mapping->table);
    }
    return $checksum;
}

echo reflectionBenchmark(100000), "\n";
