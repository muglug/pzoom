<?php
abstract class A {
    public function __construct() {
        $this->overriddenByB();
    }

    protected function overriddenByB(): void {
        // do nothing
    }
}

class B extends A {
    /** @var int */
    private $foo;

    /** @var int */
    protected $bar;

    public function __construct() {
        parent::__construct();
    }

    protected final function overriddenByB(): void {
        $this->foo = 1;
        $this->bar = 1;
    }
}
