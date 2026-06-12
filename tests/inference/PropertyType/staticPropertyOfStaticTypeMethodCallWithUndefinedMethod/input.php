<?php
class A {
    /** @var static|null */
    public $instance;

    public function foo() : void {
        if ($this->instance) {
            $this->instance->bar();
        }
    }
}

class B extends A {
    public function bar() : void {}
}
