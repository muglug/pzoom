<?php
class A {
    public array $arr = [];

    public function foo() : void {
        if (rand(0, 1)) {
            $this->arr["a"] = "hello";
        }

        if (!$this->arr) {}
    }
}
