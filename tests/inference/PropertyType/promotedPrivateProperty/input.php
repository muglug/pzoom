<?php
class A {
    public function __construct(private int $foo = 5) {}
}

echo (new A)->foo;
