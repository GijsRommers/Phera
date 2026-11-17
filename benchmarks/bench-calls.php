<?php
// Calls of many shapes, all made from inside a function, as in real code.
declare(strict_types=1);

interface Shape { public function area(): float; public function name(): string; }

abstract class Base implements Shape {
    protected static int $created = 0;
    public function __construct(protected string $label) { static::$created++; }
    public function name(): string { return $this->label; }
    public function describe(string $prefix = "shape"): string { return $prefix . ":" . $this->name(); }
    public static function created(): int { return static::$created; }
}

final class Rect extends Base {
    public function __construct(private float $w, private float $h) { parent::__construct("rect"); }
    public function area(): float { return $this->w * $this->h; }
    public function width(): float { return $this->w; }
    public function withWidth(float $w): static { return new static($w, $this->h); }
}

final class Circle extends Base {
    private const PI = 3.14159;
    public function __construct(private float $r) { parent::__construct("circle"); }
    public function area(): float { return self::PI * $this->square($this->r); }
    private function square(float $v): float { return $v * $v; }
    public function tags(): array { return [$this->label, "round"]; }
}

function clamp(int $value, int $low, int $high): int { return $value < $low ? $low : ($value > $high ? $high : $value); }
function untyped($a, $b) { return $a + $b; }
function with_default(int $a, int $step = 3, ?string $unit = null): int { return $a + $step; }
function total_area(Shape ...$shapes): float { $sum = 0.0; foreach ($shapes as $shape) { $sum += $shape->area(); } return $sum; }

function run(int $rounds): string {
    $rect = new Rect(2.0, 3.0);
    $circle = new Circle(1.5);
    $area = 0.0; $count = 0; $length = 0;
    for ($i = 0; $i < $rounds; $i++) {
        $area += $rect->area() + $circle->area();
        $count += clamp($i, 10, 1000) + untyped($i, 1) + with_default($i);
        $length += strlen($rect->describe()) + count($circle->tags());
        $area += $rect->width();
        if ($i % 16 === 0) {
            $area += total_area($rect, $circle, $rect->withWidth(4.0));
            $count += Base::created();
        }
    }
    return sprintf("%.2f|%d|%d", $area, $count, $length);
}
echo run(300000), "\n";
