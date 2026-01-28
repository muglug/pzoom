<?php
class A {
    /**
     * @readonly
     */
    public string $bar;

    public function __construct() {
        $this->bar = "hello";
    }

    public function setBar() : void {
        $this->bar = "goodbye";
    }
}
