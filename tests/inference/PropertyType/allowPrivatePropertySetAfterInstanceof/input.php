<?php
class A {
    /** @var string|null */
    private $foo;

    public function bar() : void {
        if (!$this instanceof B) {
            return;
        }

        $this->foo = "hello";
    }
}

class B extends A {}
