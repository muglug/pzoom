<?php
abstract class A {
    /** @var string */
    public $bar;

    public function __construct() {
        $this->setBar();
    }

    private function setBar(): void {
        $this->bar = "hello";
    }
}

class B extends A {
    public function __construct() {
        parent::__construct();

        echo $this->bar;
    }
}
