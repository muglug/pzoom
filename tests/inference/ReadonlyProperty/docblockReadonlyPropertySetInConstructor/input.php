<?php
class A {
    /**
     * @readonly
     */
    public string $bar;

    public function __construct() {
        $this->bar = "hello";
    }
}

echo (new A)->bar;
