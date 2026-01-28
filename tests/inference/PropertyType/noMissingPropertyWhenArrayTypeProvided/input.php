<?php

class Foo {
    private $bar;

    /** @psalm-param array{key: string} $bar */
    public function __construct(array $bar) {
        $this->bar = $bar;
    }

    public function bar(): void {
        echo $this->bar["key"];
    }
}
