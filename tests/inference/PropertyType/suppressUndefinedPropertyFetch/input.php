<?php
class A {
    public function __construct() {
        /** @psalm-suppress UndefinedThisPropertyAssignment */
        $this->bar = rand(0, 1) ? "hello" : null;
    }
}

$a = new A();
/** @psalm-suppress UndefinedPropertyFetch */
if ($a->bar === null && rand(0, 1)) {}
