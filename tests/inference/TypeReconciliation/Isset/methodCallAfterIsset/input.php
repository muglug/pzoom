<?php
class B {
    public function bar() : void {}
}

/** @psalm-suppress MissingConstructor */
class A {
    /** @var B */
    public $foo;

    public function init() : void {
        /** @psalm-suppress RedundantPropertyInitializationCheck */
        if (isset($this->foo)) {
            return;
        }

        if (rand(0, 1)) {
            $this->foo = new B;
        } else {
            $this->foo = new B;
        }

        $this->foo->bar();
    }
}