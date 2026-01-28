<?php
class A {
    private string $a;

    public function __construct(string $a) {
        $this->a = $a;
    }

    public function getA() : string {
        return $this->a;
    }
}

/** @method string getA() */
class B extends A {}
