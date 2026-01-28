<?php
class A {
    /** @var string */
    public $a = "";

    /** @var string */
    public $b = "";

    public function fooFoo(): string
    {
        list($this->a, $this->b) = ["a", "b"];

        return $this->a;
    }
}
