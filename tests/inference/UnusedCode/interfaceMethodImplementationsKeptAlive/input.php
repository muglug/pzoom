<?php

interface Shape {
    public function area(): float;
    public function perimeter(): float;
}

final class Square implements Shape {
    public function __construct(private float $side) {}

    #[Override]
    public function area(): float {
        return $this->side * $this->side;
    }

    #[Override]
    public function perimeter(): float {
        return 4 * $this->side;
    }
}

function describe(Shape $shape): string {
    return get_class($shape);
}

echo describe(new Square(2.0));
