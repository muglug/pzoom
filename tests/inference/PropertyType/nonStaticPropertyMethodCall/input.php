<?php
class A {
    /** @var self|null */
    public $instance;

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

$a = new A();

if ($a->instance) {
    $a->instance->bar();
    echo $a->instance->bat;
}
