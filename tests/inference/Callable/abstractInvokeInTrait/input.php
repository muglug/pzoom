<?php
function testFunc(callable $func) : void {}

trait TestTrait {
    abstract public function __invoke() : void;

    public function apply() : void {
        testFunc($this);
    }
}

abstract class TestClass {
    use TestTrait;
}
