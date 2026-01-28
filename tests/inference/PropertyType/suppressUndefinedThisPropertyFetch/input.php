<?php
class A {
    public function __construct() {
        /** @psalm-suppress UndefinedThisPropertyAssignment */
        $this->bar = rand(0, 1) ? "hello" : null;
    }

    /** @psalm-suppress UndefinedThisPropertyFetch */
    public function foo() : void {
        if ($this->bar === null && rand(0, 1)) {}
    }
}
