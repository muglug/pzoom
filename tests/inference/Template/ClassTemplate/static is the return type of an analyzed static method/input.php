<?php

abstract class A
{
}

final class B extends A
{
    public static function create(): static
    {
        return new self();
    }
}

final class Service
{
    public function do(): void
    {
        $this->acceptA(B::create());
    }

    private function acceptA(A $_a): void
    {
    }
}
