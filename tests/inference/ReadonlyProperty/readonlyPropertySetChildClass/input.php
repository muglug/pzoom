<?php
abstract class A {
    /**
     * @readonly
     */
    public string $bar;
}

class B extends A {
    public function __construct() {
        $this->bar = "hello";
    }
}

echo (new B)->bar;
