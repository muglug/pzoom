<?php
class A {
    public function __toString(): string
    {
        return "hello";
    }
}

/** @param string|int $b */
function fooFoo($b): void {}
fooFoo(new A());
