<?php

abstract class Shape
{
    protected float $area = 0.0;

    public function getArea(): float
    {
        return $this->area;
    }
}

final class Circle extends Shape
{
    protected float $area = 3.14;
}

echo (new Circle())->getArea();
