<?php
class MyClass {
    /**
     * @var int[]
     */
    private $prop = [];

    /**
     * @return void
     */
    public function foo() {
        array_push($this->prop, 5);
    }
}
