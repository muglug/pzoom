<?php
/**
 * @psalm-consistent-constructor
 */
class A {
    public function getInstance() : self
    {
        return new static(5);
    }

    public function __construct(int $s) {}
}

class BadAChild extends A {
    public function __construct(string $s) {}
}
