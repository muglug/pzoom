<?php
class A {
    /** @var string|null */
    public $foo;

    /** @param mixed $a */
    public function barBar($a): void
    {
        $this->foo = $a;
    }
}
