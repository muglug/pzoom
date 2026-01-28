<?php
class A {
    /** @var string[] */
    public $strs = ["a", "b", "c"];

    /** @return void */
    public function bar() {
        $this->strs[] = new stdClass(); // no issue emitted
    }
}
