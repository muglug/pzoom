<?php
class Foo {
    protected array $b;

    protected function __construct(?string ...$bb) {
        $this->b = $bb;
    }
}

class Bar extends Foo {}
