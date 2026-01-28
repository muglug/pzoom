<?php
trait T {
    public $foo = null;
}
class A {
    use T;

    public function assignToFoo(): void {
        $this->foo = 5;
    }
}
