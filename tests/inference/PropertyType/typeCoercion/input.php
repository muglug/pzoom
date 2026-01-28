<?php
class A {
    /** @var B|null */
    public $foo;

    public function barBar(A $a): void
    {
        $this->foo = $a;
    }
}

class B extends A {}
