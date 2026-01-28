<?php
class A {
    /** @var static|null */
    public $instance;
}

class B extends A {
    /** @var string|null */
    public $bat;

    public function foo() : void {
        if ($this->instance) {
            $this->instance->bar();
            echo $this->instance->bat;
        }
    }

    public function bar() : void {}
}
