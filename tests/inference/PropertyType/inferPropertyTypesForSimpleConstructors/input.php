<?php
class A {
    private $foo;
    private $bar;

    public function __construct(int $foot, string $bart) {
        $this->foo = $foot;
        $this->bar = $bart;
    }

    public function getFoo() : int {
        return $this->foo;
    }

    public function getBar() : string {
        return $this->bar;
    }
}
