<?php
class A {
    public function __toString(): string
    {
        return "hello";
    }
}

/** @param string|A $b */
function fooFoo($b): void {}

/** @param A|string $b */
function barBar($b): void {}

fooFoo(new A());
barBar(new A());
