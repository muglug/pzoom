<?php
class A {
    /** @var string */
    public $foo;

    public function __construct() {
        $this->doThing();
    }

    private function doThing(): void {
        if (rand(0, 1)) {
            $this->doOtherThing();
        }
    }

    private function doOtherThing(): void {
        if (rand(0, 1)) {
            $this->doThing();
        }
    }
}
