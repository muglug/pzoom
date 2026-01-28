<?php declare(strict_types=1);
class A {
    public function __toString(): string
    {
        return "hello";
    }
}

function fooFoo(string $b): void {}
fooFoo(new A());
