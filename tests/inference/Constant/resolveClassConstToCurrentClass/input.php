<?php
interface I {
    /** @var string|array */
    public const C = "a";

    public function getC(): string;
}

class A implements I {
    public function getC(): string {
        return self::C;
    }
}

class B extends A {
    public const C = [5];

    public function getA(): array {
        return self::C;
    }
}
