<?php
class Foo {
    private float $foo = 3.3;

    public function &foo(): float {
        return $this->foo;
    }
}
