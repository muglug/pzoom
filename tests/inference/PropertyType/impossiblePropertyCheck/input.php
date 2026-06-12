<?php
class Bar {}
class Foo {
    /** @var Bar */
    private $bar;

    public function __construct() {
        $this->bar = new Bar();
    }

    public function getBar(): void {
        if (!$this->bar) {}
    }
}
